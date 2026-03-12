use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Html;
use axum::response::sse::{Event, Sse};
use futures_core::Stream;

use crate::routes::{WebResult, watch_live_sse};
use crate::server::PageState;

use super::render::{
    remote_agents_stream_error_html, render_remote_agents_action_status,
    render_remote_agents_live_shell, render_remote_agents_page, render_remote_agents_stream_html,
};
use super::websocket::websocket_url;

pub(crate) async fn remote_agents(State(state): State<PageState>, headers: HeaderMap) -> WebResult {
    render_remote_agents_page(&state, websocket_url(&headers)).await
}

pub(crate) async fn remote_agents_events(
    State(state): State<PageState>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let websocket = websocket_url(&headers);
    let changes = state.remote_registry.subscribe_changes();
    let initial = render_remote_agents_stream_html(&state, websocket.clone()).await?;
    let render_state = state.clone();
    let render_websocket = websocket.clone();

    Ok(watch_live_sse(
        changes,
        initial,
        "opengoose-remote-agents",
        move || {
            let state = render_state.clone();
            let websocket = render_websocket.clone();
            async move { render_remote_agents_stream_html(&state, websocket).await }
        },
        remote_agents_stream_error_html(),
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

    let status_html = render_remote_agents_action_status(message.clone(), tone)?;
    let live_html = render_remote_agents_live_shell(&state, websocket_url(&headers)).await?;

    Ok(Html(format!("{status_html}{live_html}")))
}
