#![recursion_limit = "256"]

/// Dashboard view-model structs and data loaders for the HTML templates.
pub mod data;
/// Typed error types for web handlers with HTTP status code mapping.
pub mod error;
#[doc(hidden)]
pub mod fixtures;
mod handlers;
mod live;
pub mod middleware;
/// OpenAPI 3.0 spec builder and Swagger UI handler.
pub mod openapi;
mod pages;
mod routes;
/// Server configuration types (bind address, TLS paths).
pub mod server;
mod state;
#[cfg(test)]
pub(crate) mod test_support;
mod tls;

/// Re-exported error type for web API and page handlers.
pub use error::WebError;
pub use routes::render_dashboard_live_partial;
pub use server::WebOptions;
/// Re-exported shared application state for all handlers.
pub use state::AppState;
/// Alias kept for backward compatibility.
pub use state::AppState as SharedAppState;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;
use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};

use crate::handlers::remote_agents::RemoteGatewayState;
use crate::server::PageState;

#[cfg(test)]
mod tests;

/// Start the web dashboard and JSON API server.
///
/// Binds to the address in `options`, serves HTML pages, static assets,
/// REST endpoints under `/api/`, and the remote-agent WebSocket gateway.
pub async fn serve(options: WebOptions) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let remote_state = Arc::new(RemoteGatewayState {
        registry: RemoteAgentRegistry::new(RemoteConfig::default()),
    });
    let api_state = AppState::new(db.clone())?;
    let state = PageState {
        db: db.clone(),
        api_key_store: api_state.api_key_store.clone(),
        remote_registry: remote_state.registry.clone(),
        channel_metrics: api_state.channel_metrics.clone(),
        event_bus: api_state.event_bus.clone(),
    };
    live::spawn_live_event_watcher(state.db.clone(), api_state.event_bus.clone());

    let app = routes::app_router(state, api_state, remote_state);

    tls::start_server(options, app).await
}
