pub mod error;
mod handlers;
mod state;

pub use error::WebError;
pub use state::AppState;

use anyhow::Result;
use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::handlers::{agents, dashboard, runs, sessions, teams};

/// Build the Axum router with all API routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/sessions", get(sessions::list_sessions))
        .route(
            "/api/sessions/{session_key}/messages",
            get(sessions::get_messages),
        )
        .route("/api/runs", get(runs::list_runs))
        .route("/api/agents", get(agents::list_agents))
        .route("/api/teams", get(teams::list_teams))
        .route("/api/dashboard", get(dashboard::get_dashboard))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the web server on the given port.
pub async fn serve(port: u16, state: AppState) -> Result<()> {
    let app = router(state);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("opengoose web dashboard listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
