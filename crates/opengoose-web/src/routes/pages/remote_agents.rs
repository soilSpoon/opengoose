use std::convert::Infallible;
use std::time::Duration;

use askama::Template;
use async_stream::stream;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;

use crate::data::{RemoteAgentsPageView, load_remote_agents_page};
use crate::routes::{
    PartialResult, WebResult, datastar_patch_elements_event, internal_error, render_partial,
    render_template,
};
use crate::server::PageState;

pub(crate) async fn remote_agents(State(state): State<PageState>, headers: HeaderMap) -> WebResult {
    let page = load_remote_agents_page(&state.remote_registry, websocket_url(&headers))
        .await
        .map_err(internal_error)?;
    let live_html = render_partial(&RemoteAgentsLiveTemplate { page: page.clone() })?;

    render_template(&RemoteAgentsTemplate {
        page_title: "Remote Agents",
        current_nav: "remote_agents",
        page,
        live_html,
    })
}

pub(crate) async fn remote_agents_events(
    State(state): State<PageState>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let websocket = websocket_url(&headers);
    let initial = render_remote_agents_stream_html(&state, websocket.clone()).await?;
    let event_stream = stream! {
        yield Ok(datastar_patch_elements_event(&initial));

        let mut ticker = tokio::time::interval(Duration::from_secs(4));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match render_remote_agents_stream_html(&state, websocket.clone()).await {
                Ok(html) => yield Ok(datastar_patch_elements_event(&html)),
                Err(_) => yield Ok(datastar_patch_elements_event(remote_agents_stream_error_html())),
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-remote-agents"),
    ))
}

pub(crate) async fn disconnect_remote_agent(
    State(state): State<PageState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Html<String>, (StatusCode, Html<String>)> {
    let (message, tone) = if state.remote_registry.is_connected(&name).await {
        let _ = state
            .remote_registry
            .send_to(
                &name,
                opengoose_teams::remote::ProtocolMessage::Disconnect {
                    reason: "disconnected by server".into(),
                },
            )
            .await;
        state.remote_registry.unregister(&name).await;
        (format!("Disconnected {name}."), "success")
    } else {
        (format!("Agent `{name}` is no longer connected."), "danger")
    };

    let status_html = render_partial(&RemoteAgentActionStatusTemplate {
        message: message.clone(),
        tone,
    })?;
    let live_html = render_remote_agents_live_html(&state, websocket_url(&headers)).await?;

    Ok(Html(format!("{status_html}{live_html}")))
}

pub(crate) fn websocket_url(headers: &HeaderMap) -> String {
    let host = forwarded_header(headers, "x-forwarded-host")
        .or_else(|| forwarded_host(headers))
        .or_else(|| header_string(headers, "host"))
        .unwrap_or_else(|| "localhost:3000".into());
    let scheme = match forwarded_header(headers, "x-forwarded-proto")
        .or_else(|| forwarded_proto(headers))
        .as_deref()
    {
        Some("https") | Some("wss") => "wss",
        _ => "ws",
    };

    format!("{scheme}://{host}/api/agents/connect")
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').next().unwrap_or(value).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn forwarded_header(headers: &HeaderMap, name: &str) -> Option<String> {
    header_string(headers, name)
}

fn forwarded_proto(headers: &HeaderMap) -> Option<String> {
    headers
        .get("forwarded")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .find_map(|segment| segment.trim().strip_prefix("proto="))
        })
        .map(|value| value.trim_matches('"').to_string())
}

fn forwarded_host(headers: &HeaderMap) -> Option<String> {
    headers
        .get("forwarded")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .find_map(|segment| segment.trim().strip_prefix("host="))
        })
        .map(|value| value.trim_matches('"').to_string())
}

#[derive(Template)]
#[template(path = "remote_agents.html")]
struct RemoteAgentsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: RemoteAgentsPageView,
    live_html: String,
}

#[derive(Template)]
#[template(path = "partials/remote_agents_live.html")]
struct RemoteAgentsLiveTemplate {
    page: RemoteAgentsPageView,
}

#[derive(Template)]
#[template(path = "partials/remote_agents_action_status.html")]
struct RemoteAgentActionStatusTemplate {
    message: String,
    tone: &'static str,
}

async fn render_remote_agents_live_html(state: &PageState, websocket: String) -> PartialResult {
    let page = load_remote_agents_page(&state.remote_registry, websocket)
        .await
        .map_err(internal_error)?;
    let live_html = render_partial(&RemoteAgentsLiveTemplate { page })?;
    Ok(format!(r#"<div id="remote-agents-live">{live_html}</div>"#))
}

async fn render_remote_agents_stream_html(state: &PageState, websocket: String) -> PartialResult {
    render_remote_agents_live_html(state, websocket).await
}

fn remote_agents_stream_error_html() -> &'static str {
    r#"
<div id="remote-agents-live">
  <section class="callout tone-danger">
    <p class="eyebrow">Registry unavailable</p>
    <h2>Remote agent snapshot unavailable</h2>
    <p>The registry will retry on the next refresh interval.</p>
  </section>
</div>
"#
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn render_remote_agents_live(page: RemoteAgentsPageView) -> PartialResult {
        render_partial(&RemoteAgentsLiveTemplate { page })
    }
}
