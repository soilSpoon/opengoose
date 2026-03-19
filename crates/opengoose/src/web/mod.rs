mod api;
mod pages;
mod sse;

use std::sync::Arc;

use axum::Router;
use opengoose_board::db_board::DbBoard;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub board: Arc<DbBoard>,
    pub tx: broadcast::Sender<()>,
}

/// 웹 서버를 백그라운드 task로 시작. TUI/headless와 동시에 동작.
pub async fn spawn_server(board: Arc<DbBoard>, port: u16) -> anyhow::Result<()> {
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
        .route("/api/board", axum::routing::get(api::board_list).post(api::board_create))
        .route("/api/board/{id}", axum::routing::get(api::board_get))
        .route("/api/board/{id}/claim", axum::routing::post(api::board_claim))
        .route("/api/rigs", axum::routing::get(api::rigs_list))
        .route("/api/rigs/{id}", axum::routing::get(api::rig_detail))
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
