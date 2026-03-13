use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use opengoose_core::{Engine, GatewayBridge};
use opengoose_persistence::Database;
use opengoose_types::EventBus;

use super::TelegramGateway;

#[derive(Debug, Clone)]
pub(crate) struct RecordedRequest {
    pub(crate) path: String,
    pub(crate) body: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(crate) struct MockResponse {
    body: String,
}

impl MockResponse {
    pub(crate) fn json(body: serde_json::Value) -> Self {
        Self {
            body: body.to_string(),
        }
    }

    pub(crate) fn raw(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }
}

pub(crate) struct MockTelegramApi {
    pub(crate) base_url: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockTelegramApi {
    pub(crate) async fn spawn(responses: Vec<MockResponse>) -> Self {
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
                    .unwrap_or_else(|| MockResponse::json(serde_json::json!({})));

                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response.body.len(),
                    response.body,
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

    pub(crate) fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub(crate) async fn wait_for_requests(&self, expected_count: usize) {
        let wait = async {
            loop {
                if self.requests.lock().unwrap().len() >= expected_count {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };

        tokio::time::timeout(Duration::from_secs(1), wait)
            .await
            .expect("timed out waiting for mock Telegram requests");
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

pub(crate) fn test_gateway(api_base_url: &str, event_bus: EventBus) -> TelegramGateway {
    let bridge = Arc::new(GatewayBridge::new(Arc::new(Engine::new(
        EventBus::new(16),
        Database::open_in_memory().unwrap(),
    ))));

    TelegramGateway::with_api_base_url("test-token", bridge, event_bus, api_base_url).unwrap()
}
