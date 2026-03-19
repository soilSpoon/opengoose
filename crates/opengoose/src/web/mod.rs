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

pub async fn serve(board: Arc<DbBoard>, port: u16) -> anyhow::Result<()> {
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
        .route("/pico.min.css", axum::routing::get(pages::pico_css))
        .route("/alpine.min.js", axum::routing::get(pages::alpine_js))
        .route("/api/board", axum::routing::get(api::board_list))
        .route("/api/board/{id}", axum::routing::get(api::board_get))
        .route("/api/rigs", axum::routing::get(api::rigs_list))
        .route("/api/events", axum::routing::get(sse::events))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!("dashboard at http://localhost:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}
