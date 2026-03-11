use std::sync::Arc;

use askama::Template;
use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::Event;
use tower_http::services::ServeDir;

use crate::AppState;
use crate::handlers::remote_agents::RemoteGatewayState;
use crate::pages::not_found_handler;
use crate::server::PageState;

mod api;
pub(crate) mod health;
mod live;
mod pages;

pub(crate) use live::{broadcast_live_sse, watch_live_sse};
pub use pages::render_dashboard_live_partial;

type WebResult = Result<Html<String>, (StatusCode, Html<String>)>;
type PartialResult = Result<String, (StatusCode, Html<String>)>;
type ApiResult<T> = Result<Json<T>, (StatusCode, Json<serde_json::Value>)>;

pub(crate) fn app_router(
    page_state: PageState,
    api_state: AppState,
    remote_state: Arc<RemoteGatewayState>,
) -> Router {
    pages::router(page_state.clone())
        .merge(health::page_router(page_state))
        .merge(api::router(api_state))
        .merge(api::remote_router(remote_state))
        .nest_service(
            "/assets",
            ServeDir::new(format!("{}/assets", env!("CARGO_MANIFEST_DIR"))),
        )
        .fallback(not_found_handler)
}

fn internal_error(error: anyhow::Error) -> (StatusCode, Html<String>) {
    let page = crate::pages::ErrorPage::internal_error(&error.to_string());
    let html = page
        .render()
        .unwrap_or_else(|_| format!("<p>Internal Server Error: {error}</p>"));
    (StatusCode::INTERNAL_SERVER_ERROR, Html(html))
}

fn render_partial<T: Template>(template: &T) -> PartialResult {
    template
        .render()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, Html(error.to_string())))
}

fn render_template<T: Template>(template: &T) -> WebResult {
    render_partial(template).map(Html)
}

pub(crate) fn datastar_patch_elements_event(html: &str) -> Event {
    let mut payload = String::new();

    if html.is_empty() {
        payload.push_str("elements ");
    } else {
        for line in html.lines() {
            if !payload.is_empty() {
                payload.push('\n');
            }
            payload.push_str("elements ");
            payload.push_str(line);
        }
    }

    Event::default()
        .event("datastar-patch-elements")
        .data(payload)
}

pub(crate) fn api_error(
    status: StatusCode,
    message: impl std::fmt::Display,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "error": message.to_string() })),
    )
}

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use super::pages::test_support::*;
}
