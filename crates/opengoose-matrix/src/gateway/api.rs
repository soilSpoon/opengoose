use std::sync::atomic::Ordering;

use crate::types::{MatrixError, SendEventResponse, SyncFilter, SyncResponse, WhoAmI};

use super::{MatrixGateway, SYNC_TIMEOUT_MS, urlencoding};

impl MatrixGateway {
    pub(super) fn v3_url(&self, path: &str) -> String {
        format!("{}/_matrix/client/v3{}", self.homeserver_url, path)
    }

    pub(super) fn next_txn_id(&self) -> String {
        let n = self.txn_counter.fetch_add(1, Ordering::Relaxed);
        format!("opengoose-{}-{}", std::process::id(), n)
    }

    pub(super) fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    /// GET /account/whoami — returns the bot's Matrix user ID.
    pub(super) async fn whoami(&self) -> anyhow::Result<String> {
        let resp: WhoAmI = self
            .client
            .get(self.v3_url("/account/whoami"))
            .header("Authorization", self.auth_header())
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.user_id)
    }

    /// Register a minimal sync filter and return the filter ID.
    pub(super) async fn register_filter(&self, user_id: &str) -> anyhow::Result<String> {
        let encoded_user = urlencoding::encode(user_id).into_owned();
        let filter = SyncFilter::messages_only();
        let resp: serde_json::Value = self
            .client
            .post(self.v3_url(&format!("/user/{encoded_user}/filter")))
            .header("Authorization", self.auth_header())
            .json(&filter)
            .send()
            .await?
            .json()
            .await?;
        resp.get("filter_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no filter_id in response"))
    }

    /// GET /sync — long-poll for new events.
    pub(super) async fn sync(
        &self,
        since: Option<&str>,
        filter_id: Option<&str>,
    ) -> anyhow::Result<SyncResponse> {
        let mut req = self
            .client
            .get(self.v3_url("/sync"))
            .header("Authorization", self.auth_header())
            .query(&[("timeout", SYNC_TIMEOUT_MS.to_string())]);

        if let Some(s) = since {
            req = req.query(&[("since", s)]);
        }
        if let Some(f) = filter_id {
            req = req.query(&[("filter", f)]);
        }

        Ok(req.send().await?.json().await?)
    }

    /// PUT /rooms/{roomId}/send/{eventType}/{txnId} — send a message event.
    pub(super) async fn send_event(
        &self,
        room_id: &str,
        content: &serde_json::Value,
    ) -> anyhow::Result<String> {
        let encoded_room = urlencoding::encode(room_id).into_owned();
        let txn_id = self.next_txn_id();
        let url = self.v3_url(&format!(
            "/rooms/{encoded_room}/send/m.room.message/{txn_id}"
        ));

        let resp = self
            .client
            .put(&url)
            .header("Authorization", self.auth_header())
            .json(content)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: MatrixError = resp.json().await.unwrap_or(MatrixError {
                errcode: None,
                error: Some("unknown error".into()),
            });
            anyhow::bail!(
                "send_event failed: {} — {}",
                err.errcode.unwrap_or_default(),
                err.error.unwrap_or_default()
            );
        }

        let ev: SendEventResponse = resp.json().await?;
        Ok(ev.event_id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use opengoose_core::{Engine, GatewayBridge};
    use opengoose_persistence::Database;
    use opengoose_types::EventBus;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct RecordedRequest {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: serde_json::Value,
    }

    #[derive(Debug, Clone)]
    struct MockResponse {
        status: &'static str,
        body: serde_json::Value,
    }

    struct MockMatrixApi {
        base_url: String,
        requests: Arc<Mutex<Vec<RecordedRequest>>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl MockMatrixApi {
        async fn spawn(responses: Vec<MockResponse>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let response_queue = Arc::new(Mutex::new(VecDeque::from(responses)));
            let request_log = Arc::clone(&requests);
            let queued_responses = Arc::clone(&response_queue);

            let task = tokio::spawn(async move {
                loop {
                    let (mut socket, _) = match listener.accept().await {
                        Ok(connection) => connection,
                        Err(_) => break,
                    };

                    let request = match read_request(&mut socket).await {
                        Ok(request) => request,
                        Err(_) => continue,
                    };
                    request_log.lock().unwrap().push(request);

                    let response =
                        queued_responses
                            .lock()
                            .unwrap()
                            .pop_front()
                            .unwrap_or(MockResponse {
                                status: "200 OK",
                                body: serde_json::json!({}),
                            });

                    let body = response.body.to_string();
                    let reply = format!(
                        "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        response.status,
                        body.len(),
                        body
                    );

                    let _ = socket.write_all(reply.as_bytes()).await;
                }
            });

            Self {
                base_url: format!("http://{addr}"),
                requests,
                task,
            }
        }

        fn requests(&self) -> Vec<RecordedRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for MockMatrixApi {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn read_request(socket: &mut TcpStream) -> anyhow::Result<RecordedRequest> {
        let mut buffer = Vec::new();
        let mut chunk = [0; 1024];

        loop {
            let bytes_read = socket.read(&mut chunk).await?;
            if bytes_read == 0 {
                anyhow::bail!("client closed connection before request completed");
            }

            buffer.extend_from_slice(&chunk[..bytes_read]);
            let Some(header_end) = find_header_end(&buffer) else {
                continue;
            };

            let headers = std::str::from_utf8(&buffer[..header_end])?;
            let mut header_lines = headers.lines();
            let request_line = header_lines
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing request line"))?;
            let mut request_parts = request_line.split_whitespace();
            let method = request_parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing request method"))?
                .to_string();
            let path = request_parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing request path"))?
                .to_string();

            let mut parsed_headers = HashMap::new();
            let mut content_length = 0usize;
            for line in header_lines {
                let Some((name, value)) = line.split_once(':') else {
                    continue;
                };
                let key = name.trim().to_ascii_lowercase();
                let value = value.trim().to_string();
                if key == "content-length" {
                    content_length = value.parse()?;
                }
                parsed_headers.insert(key, value);
            }

            let body_start = header_end + 4;
            if buffer.len() < body_start + content_length {
                continue;
            }

            let body = if content_length == 0 {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&buffer[body_start..body_start + content_length])?
            };

            return Ok(RecordedRequest {
                method,
                path,
                headers: parsed_headers,
                body,
            });
        }
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn test_gateway(base_url: &str) -> MatrixGateway {
        let bridge = Arc::new(GatewayBridge::new(Arc::new(Engine::new(
            EventBus::new(16),
            Database::open_in_memory().unwrap(),
        ))));

        MatrixGateway::new(base_url, "test-access-token", bridge, EventBus::new(16)).unwrap()
    }

    fn query_params(path: &str) -> HashMap<&str, &str> {
        path.split_once('?')
            .map(|(_, query)| {
                query
                    .split('&')
                    .filter_map(|pair| pair.split_once('='))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn v3_url_joins_base_url_and_path_without_double_slash() {
        let gateway = test_gateway("https://matrix.example.com/");
        assert_eq!(
            gateway.v3_url("/sync"),
            "https://matrix.example.com/_matrix/client/v3/sync"
        );
    }

    #[test]
    fn next_txn_id_is_monotonic_for_a_gateway_instance() {
        let gateway = test_gateway("https://matrix.example.com");
        let first = gateway.next_txn_id();
        let second = gateway.next_txn_id();

        assert_ne!(first, second);
        assert!(first.ends_with("-0"));
        assert!(second.ends_with("-1"));
    }

    #[test]
    fn auth_header_formats_bearer_token() {
        let gateway = test_gateway("https://matrix.example.com");
        assert_eq!(gateway.auth_header(), "Bearer test-access-token");
    }

    #[tokio::test]
    async fn whoami_uses_v3_endpoint_and_auth_header() {
        let server = MockMatrixApi::spawn(vec![MockResponse {
            status: "200 OK",
            body: serde_json::json!({ "user_id": "@bot:example.com" }),
        }])
        .await;
        let gateway = test_gateway(&server.base_url);

        let user_id = gateway.whoami().await.unwrap();
        let requests = server.requests();

        assert_eq!(user_id, "@bot:example.com");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].path, "/_matrix/client/v3/account/whoami");
        assert_eq!(
            requests[0].headers.get("authorization").map(String::as_str),
            Some("Bearer test-access-token")
        );
    }

    #[tokio::test]
    async fn register_filter_encodes_user_id_and_posts_messages_only_filter() {
        let server = MockMatrixApi::spawn(vec![MockResponse {
            status: "200 OK",
            body: serde_json::json!({ "filter_id": "filter-123" }),
        }])
        .await;
        let gateway = test_gateway(&server.base_url);

        let filter_id = gateway.register_filter("@bot:example.com").await.unwrap();
        let requests = server.requests();

        assert_eq!(filter_id, "filter-123");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(
            requests[0].path,
            format!(
                "/_matrix/client/v3/user/{}/filter",
                urlencoding::encode("@bot:example.com").into_owned()
            )
        );
        assert_eq!(
            requests[0].headers.get("authorization").map(String::as_str),
            Some("Bearer test-access-token")
        );
        assert_eq!(
            requests[0].body,
            serde_json::to_value(SyncFilter::messages_only()).unwrap()
        );
    }

    #[tokio::test]
    async fn register_filter_errors_when_filter_id_is_missing() {
        let server = MockMatrixApi::spawn(vec![MockResponse {
            status: "200 OK",
            body: serde_json::json!({}),
        }])
        .await;
        let gateway = test_gateway(&server.base_url);

        let error = gateway
            .register_filter("@bot:example.com")
            .await
            .unwrap_err();

        assert!(error.to_string().contains("no filter_id in response"));
    }

    #[tokio::test]
    async fn sync_adds_timeout_since_and_filter_query_params() {
        let server = MockMatrixApi::spawn(vec![MockResponse {
            status: "200 OK",
            body: serde_json::json!({ "next_batch": "s123" }),
        }])
        .await;
        let gateway = test_gateway(&server.base_url);

        let response = gateway
            .sync(Some("since-1"), Some("filter-1"))
            .await
            .unwrap();
        let requests = server.requests();
        let params = query_params(&requests[0].path);

        assert_eq!(response.next_batch, "s123");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(
            requests[0].path.split('?').next(),
            Some("/_matrix/client/v3/sync")
        );
        assert_eq!(
            requests[0].headers.get("authorization").map(String::as_str),
            Some("Bearer test-access-token")
        );
        assert_eq!(params.get("timeout"), Some(&"30000"));
        assert_eq!(params.get("since"), Some(&"since-1"));
        assert_eq!(params.get("filter"), Some(&"filter-1"));
    }

    #[tokio::test]
    async fn sync_omits_optional_query_params_when_not_provided() {
        let server = MockMatrixApi::spawn(vec![MockResponse {
            status: "200 OK",
            body: serde_json::json!({ "next_batch": "s999" }),
        }])
        .await;
        let gateway = test_gateway(&server.base_url);

        let _ = gateway.sync(None, None).await.unwrap();
        let requests = server.requests();
        let params = query_params(&requests[0].path);

        assert_eq!(requests.len(), 1);
        assert_eq!(params.get("timeout"), Some(&"30000"));
        assert_eq!(params.get("since"), None);
        assert_eq!(params.get("filter"), None);
    }

    #[tokio::test]
    async fn send_event_encodes_room_id_and_returns_event_id() {
        let server = MockMatrixApi::spawn(vec![MockResponse {
            status: "200 OK",
            body: serde_json::json!({ "event_id": "$event:example.com" }),
        }])
        .await;
        let gateway = test_gateway(&server.base_url);
        let content = serde_json::json!({
            "msgtype": "m.text",
            "body": "hello matrix",
        });

        let event_id = gateway
            .send_event("!room:example.com", &content)
            .await
            .unwrap();
        let requests = server.requests();
        let request = &requests[0];

        assert_eq!(event_id, "$event:example.com");
        assert_eq!(requests.len(), 1);
        assert_eq!(request.method, "PUT");
        assert_eq!(
            request.headers.get("authorization").map(String::as_str),
            Some("Bearer test-access-token")
        );
        assert_eq!(request.body, content);
        assert!(
            request.path.starts_with(
                "/_matrix/client/v3/rooms/%21room%3Aexample.com/send/m.room.message/opengoose-"
            ),
            "unexpected path: {}",
            request.path
        );
        assert!(
            request.path.ends_with("-0"),
            "unexpected path: {}",
            request.path
        );
    }

    #[tokio::test]
    async fn send_event_surfaces_matrix_error_details() {
        let server = MockMatrixApi::spawn(vec![MockResponse {
            status: "403 Forbidden",
            body: serde_json::json!({
                "errcode": "M_FORBIDDEN",
                "error": "denied",
            }),
        }])
        .await;
        let gateway = test_gateway(&server.base_url);

        let error = gateway
            .send_event(
                "!room:example.com",
                &serde_json::json!({ "msgtype": "m.text", "body": "nope" }),
            )
            .await
            .unwrap_err();
        let message = error.to_string();

        assert!(message.contains("send_event failed"));
        assert!(message.contains("M_FORBIDDEN"));
        assert!(message.contains("denied"));
    }
}
