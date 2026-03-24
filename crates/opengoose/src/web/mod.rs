mod api;
mod pages;
mod sse;

use std::sync::Arc;

use axum::Router;
use opengoose_board::Board;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub board: Arc<Board>,
    pub tx: broadcast::Sender<()>,
}

/// 웹 서버를 백그라운드 task로 시작. TUI/headless와 동시에 동작.
pub async fn spawn_server(board: Arc<Board>, port: u16) -> anyhow::Result<()> {
    let (tx, _) = broadcast::channel::<()>(64);

    // Notify → broadcast bridge
    let notify = board.notify_handle();
    let tx2 = tx.clone();
    tokio::spawn(async move {
        loop {
            notify.notified().await;
            let _ = tx2.send(());
        }
    });

    let state = AppState { board, tx };

    let app = Router::new()
        .route("/", axum::routing::get(pages::index))
        .route(
            "/api/board",
            axum::routing::get(api::board_list).post(api::board_create),
        )
        .route("/api/board/{id}", axum::routing::get(api::board_get))
        .route(
            "/api/board/{id}/claim",
            axum::routing::post(api::board_claim),
        )
        .route("/api/rigs", axum::routing::get(api::rigs_list))
        .route("/api/rigs/{id}", axum::routing::get(api::rig_detail))
        .route("/api/skills", axum::routing::get(api::skills_list))
        .route(
            "/api/skills/{name}",
            axum::routing::get(api::skill_detail).delete(api::skill_delete),
        )
        .route(
            "/api/skills/{name}/promote",
            axum::routing::post(api::skill_promote),
        )
        .route("/api/events", axum::routing::get(sse::events))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    tracing::info!("dashboard at http://localhost:{port}");
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("web server error: {e}");
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_board::{PostWorkItem, Priority, RigId};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn connect_with_retry(port: u16) -> tokio::net::TcpStream {
        let start = std::time::Instant::now();
        loop {
            match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                Ok(s) => return s,
                Err(_) if start.elapsed() < std::time::Duration::from_secs(2) => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(e) => panic!("server did not start within 2s: {e}"),
            }
        }
    }

    #[tokio::test]
    async fn spawn_server_binds_and_serves_index() {
        let board = Arc::new(
            Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("async operation should succeed");
        let port = listener
            .local_addr()
            .expect("operation should succeed")
            .port();
        drop(listener);

        spawn_server(board.clone(), port)
            .await
            .expect("async operation should succeed");

        let mut stream = connect_with_retry(port).await;
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .await
            .expect("operation should succeed");
        let mut buf = String::new();
        stream
            .read_to_string(&mut buf)
            .await
            .expect("async operation should succeed");

        assert!(buf.contains("HTTP/1.1 200"));
        assert!(buf.contains("OpenGoose Dashboard"));
    }

    /// Covers web/mod.rs:27 — the notify→broadcast bridge fires tx2.send(()) when board is mutated.
    /// Also covers sse.rs:14 — the Ok(()) arm of the filter_map closure in events().
    #[tokio::test]
    async fn board_notify_triggers_sse_event() {
        let board = Arc::new(
            Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("async operation should succeed");
        let port = listener
            .local_addr()
            .expect("operation should succeed")
            .port();
        drop(listener);

        spawn_server(board.clone(), port)
            .await
            .expect("async operation should succeed");

        // Open an SSE connection (keep-alive, not close)
        let mut sse_stream = connect_with_retry(port).await;
        sse_stream
            .write_all(
                b"GET /api/events HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\n\r\n",
            )
            .await
            .expect("operation should succeed");

        // Read the HTTP response headers
        let mut header_buf = vec![0u8; 2048];
        let n = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            sse_stream.read(&mut header_buf),
        )
        .await
        .expect("timeout reading SSE headers")
        .expect("operation should succeed");
        let headers = String::from_utf8_lossy(&header_buf[..n]);
        assert!(
            headers.contains("200") || headers.contains("event-stream"),
            "expected SSE response, got: {headers}"
        );

        // Trigger a board notification by posting a work item
        board
            .post(PostWorkItem {
                title: "SSE trigger test".into(),
                description: String::new(),
                created_by: RigId::new("human"),
                priority: Priority::P2,
                tags: vec![],
            })
            .await
            .expect("operation should succeed");

        // Read the SSE event (with generous timeout for async scheduling)
        let mut event_buf = vec![0u8; 1024];
        let n = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            sse_stream.read(&mut event_buf),
        )
        .await
        .expect("timeout waiting for SSE event")
        .expect("operation should succeed");

        let event = String::from_utf8_lossy(&event_buf[..n]);
        assert!(
            event.contains("board_changed"),
            "expected board_changed SSE event, got: {event}"
        );
    }
}
