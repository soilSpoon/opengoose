//! `StreamResponder` implementation for Telegram draft-based streaming.

use async_trait::async_trait;
use tracing::debug;

use opengoose_core::message_utils::truncate_for_display;
use opengoose_core::{DraftHandle, StreamResponder};

use super::types::{SentMessage, TelegramResponse};
use super::{TELEGRAM_MAX_LEN, TelegramGateway};

#[async_trait]
impl StreamResponder for TelegramGateway {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_message_len(&self) -> usize {
        TELEGRAM_MAX_LEN
    }

    async fn create_draft(&self, chat_id: &str) -> anyhow::Result<DraftHandle> {
        debug!(chat_id = %chat_id, "creating telegram draft");
        let resp: TelegramResponse<SentMessage> = self
            .client
            .post(self.api_url("sendMessage"))
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": "Thinking...",
            }))
            .send()
            .await?
            .json()
            .await?;

        let msg = resp
            .result
            .ok_or_else(|| anyhow::anyhow!("sendMessage returned no result"))?;
        debug!(chat_id = %chat_id, message_id = msg.message_id, "telegram draft created");
        Ok(DraftHandle {
            message_id: msg.message_id.to_string(),
            channel_id: chat_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        debug!(chat_id = %handle.channel_id, message_id = %handle.message_id, content_len = content.len(), "updating telegram draft");
        let display = truncate_for_display(content, TELEGRAM_MAX_LEN);
        let _: TelegramResponse<serde_json::Value> = self
            .client
            .post(self.api_url("editMessageText"))
            .json(&serde_json::json!({
                "chat_id": handle.channel_id,
                "message_id": handle.message_id.parse::<i64>()?,
                "text": display,
            }))
            .send()
            .await?
            .json()
            .await?;
        Ok(())
    }

    async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()> {
        self.client
            .post(self.api_url("sendMessage"))
            .json(&serde_json::json!({
                "chat_id": channel_id,
                "text": content,
            }))
            .send()
            .await?;
        Ok(())
    }

    // finalize_draft uses the default implementation from StreamResponder
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use opengoose_core::{Engine, GatewayBridge, StreamResponder};
    use opengoose_persistence::Database;
    use opengoose_types::EventBus;

    use super::*;

    #[derive(Debug, Clone)]
    struct RecordedRequest {
        path: String,
        body: serde_json::Value,
    }

    struct MockTelegramApi {
        base_url: String,
        requests: Arc<Mutex<Vec<RecordedRequest>>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl MockTelegramApi {
        async fn spawn(responses: Vec<serde_json::Value>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let queued_responses = Arc::new(Mutex::new(VecDeque::from(responses)));
            let request_log = requests.clone();
            let response_queue = queued_responses.clone();

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

                    let response = response_queue
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or_else(|| serde_json::json!({}));

                    let body = response.to_string();
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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

    impl Drop for MockTelegramApi {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> anyhow::Result<RecordedRequest> {
        let mut buffer = Vec::new();
        let mut chunk = [0; 1024];

        loop {
            let bytes_read = socket.read(&mut chunk).await?;
            if bytes_read == 0 {
                anyhow::bail!("client closed connection before request completed");
            }

            buffer.extend_from_slice(&chunk[..bytes_read]);
            if let Some(header_end) = find_header_end(&buffer) {
                let headers = std::str::from_utf8(&buffer[..header_end])?;
                let path = headers
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .ok_or_else(|| anyhow::anyhow!("missing request path"))?
                    .to_string();
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        (name.eq_ignore_ascii_case("content-length"))
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                let body_start = header_end + 4;
                if buffer.len() < body_start + content_length {
                    continue;
                }

                let body = if content_length == 0 {
                    serde_json::Value::Null
                } else {
                    serde_json::from_slice(&buffer[body_start..body_start + content_length])?
                };

                return Ok(RecordedRequest { path, body });
            }
        }
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn test_gateway(api_base_url: &str) -> TelegramGateway {
        let bridge = Arc::new(GatewayBridge::new(Arc::new(Engine::new(
            EventBus::new(16),
            Database::open_in_memory().unwrap(),
        ))));

        TelegramGateway::with_api_base_url("test-token", bridge, EventBus::new(16), api_base_url)
            .unwrap()
    }

    #[tokio::test]
    async fn create_draft_posts_placeholder_and_returns_handle() {
        let api = MockTelegramApi::spawn(vec![serde_json::json!({
            "ok": true,
            "result": { "message_id": 42 }
        })])
        .await;
        let gateway = test_gateway(&api.base_url);

        let handle = gateway.create_draft("123").await.unwrap();

        assert_eq!(handle.message_id, "42");
        assert_eq!(handle.channel_id, "123");

        let requests = api.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/bottest-token/sendMessage");
        assert_eq!(requests[0].body["chat_id"], "123");
        assert_eq!(requests[0].body["text"], "Thinking...");
    }

    #[tokio::test]
    async fn update_draft_truncates_content_before_editing() {
        let api = MockTelegramApi::spawn(vec![serde_json::json!({
            "ok": true,
            "result": {}
        })])
        .await;
        let gateway = test_gateway(&api.base_url);
        let handle = DraftHandle {
            message_id: "42".to_string(),
            channel_id: "123".to_string(),
        };
        let content = format!("{}🙂tail", "a".repeat(TELEGRAM_MAX_LEN - 1));

        gateway.update_draft(&handle, &content).await.unwrap();

        let requests = api.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/bottest-token/editMessageText");
        assert_eq!(requests[0].body["chat_id"], "123");
        assert_eq!(requests[0].body["message_id"], 42);
        assert_eq!(
            requests[0].body["text"].as_str().unwrap(),
            "a".repeat(TELEGRAM_MAX_LEN - 1)
        );
    }

    #[tokio::test]
    async fn finalize_draft_updates_first_chunk_and_sends_overflow_message() {
        let api = MockTelegramApi::spawn(vec![
            serde_json::json!({ "ok": true, "result": {} }),
            serde_json::json!({}),
        ])
        .await;
        let gateway = test_gateway(&api.base_url);
        let handle = DraftHandle {
            message_id: "42".to_string(),
            channel_id: "123".to_string(),
        };
        let content = format!("{}\n{}", "a".repeat(TELEGRAM_MAX_LEN - 1), "overflow");

        gateway.finalize_draft(&handle, &content).await.unwrap();

        let requests = api.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].path, "/bottest-token/editMessageText");
        assert_eq!(
            requests[0].body["text"].as_str().unwrap(),
            "a".repeat(TELEGRAM_MAX_LEN - 1)
        );
        assert_eq!(requests[1].path, "/bottest-token/sendMessage");
        assert_eq!(requests[1].body["chat_id"], "123");
        assert_eq!(requests[1].body["text"], "overflow");
    }
}
