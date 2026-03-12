use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use askama::Template;
use async_stream::stream;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use opengoose_persistence::Database;

use crate::data::{AgentMapView, load_agent_map};
use crate::routes::{
    PartialResult, WebResult, datastar_patch_elements_event, internal_error, render_partial,
    render_template,
};
use crate::server::PageState;

pub(crate) async fn agent_map(State(state): State<PageState>) -> WebResult {
    let page = load_agent_map(state.db.clone()).map_err(internal_error)?;
    let live_html = render_partial(&AgentMapLiveTemplate { page: page.clone() })?;
    render_template(&AgentMapTemplate {
        page_title: "Agent Map",
        current_nav: "agent-map",
        page,
        live_html,
    })
}

pub(crate) async fn agent_map_events(
    State(state): State<PageState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let db = state.db;
    let initial = render_agent_map_stream_html(db.clone())?;
    let event_stream = stream! {
        yield Ok(datastar_patch_elements_event(&initial));

        let mut ticker = tokio::time::interval(Duration::from_secs(2));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match render_agent_map_stream_html(db.clone()) {
                Ok(html) => yield Ok(datastar_patch_elements_event(&html)),
                Err(_) => {
                    yield Ok(datastar_patch_elements_event(agent_map_stream_error_html()));
                }
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-agent-map"),
    ))
}

fn render_agent_map_stream_html(db: Arc<Database>) -> PartialResult {
    let page = load_agent_map(db).map_err(internal_error)?;
    let live_html = render_partial(&AgentMapLiveTemplate { page })?;
    Ok(format!(r#"<div id="agent-map-live">{live_html}</div>"#))
}

fn agent_map_stream_error_html() -> &'static str {
    r#"
<div id="agent-map-live">
  <section class="callout tone-danger">
    <p class="eyebrow">Stream degraded</p>
    <h2>Agent map snapshot unavailable</h2>
    <p>The live stream is retrying in the background.</p>
  </section>
</div>
"#
}

#[derive(Template)]
#[template(path = "agent_map.html")]
struct AgentMapTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: AgentMapView,
    live_html: String,
}

#[derive(Template)]
#[template(path = "partials/agent_map_live.html")]
struct AgentMapLiveTemplate {
    page: AgentMapView,
}
