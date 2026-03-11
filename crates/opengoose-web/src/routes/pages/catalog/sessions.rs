use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use opengoose_types::AppEventKind;
use serde::Deserialize;

use crate::data::load_sessions_page;
use crate::routes::{WebResult, datastar_patch_elements_event, internal_error};
use crate::server::PageState;

use super::render::{render_sessions_page, render_sessions_stream_html};

#[derive(Deserialize, Default)]
pub(crate) struct SessionQuery {
    pub(crate) session: Option<String>,
}

pub(crate) async fn sessions(
    State(state): State<PageState>,
    Query(query): Query<SessionQuery>,
) -> WebResult {
    let page = load_sessions_page(state.db, query.session).map_err(internal_error)?;
    render_sessions_page(page)
}

pub(crate) async fn sessions_events(
    State(state): State<PageState>,
    Query(query): Query<SessionQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let db = state.db;
    let selected = query.session;
    let mut rx = state.event_bus.subscribe();
    let initial = render_sessions_stream_html(db.clone(), selected.clone())?;

    let event_stream = stream! {
        yield Ok(datastar_patch_elements_event(&initial));

        loop {
            match rx.recv().await {
                Ok(app_event) if matches_sessions_live_event(&app_event.kind) => {
                    match render_sessions_stream_html(db.clone(), selected.clone()) {
                        Ok(html) => yield Ok(datastar_patch_elements_event(&html)),
                        Err(_) => yield Ok(datastar_patch_elements_event(sessions_stream_error_html())),
                    }
                }
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-sessions"),
    ))
}

fn matches_sessions_live_event(kind: &AppEventKind) -> bool {
    matches!(
        kind,
        AppEventKind::SessionUpdated { .. }
            | AppEventKind::MessageReceived { .. }
            | AppEventKind::ResponseSent { .. }
            | AppEventKind::PairingCompleted { .. }
            | AppEventKind::TeamActivated { .. }
            | AppEventKind::TeamDeactivated { .. }
            | AppEventKind::SessionDisconnected { .. }
            | AppEventKind::StreamStarted { .. }
            | AppEventKind::StreamUpdated { .. }
            | AppEventKind::StreamCompleted { .. }
            | AppEventKind::RunUpdated { .. }
            | AppEventKind::TeamRunStarted { .. }
            | AppEventKind::TeamStepStarted { .. }
            | AppEventKind::TeamStepCompleted { .. }
            | AppEventKind::TeamStepFailed { .. }
            | AppEventKind::TeamRunCompleted { .. }
            | AppEventKind::TeamRunFailed { .. }
    )
}

fn sessions_stream_error_html() -> &'static str {
    r#"
<section id="detail-shell" class="detail-shell">
  <section class="detail-frame">
    <section class="callout tone-danger">
      <p class="eyebrow">Session stream degraded</p>
      <h2>Live session updates paused</h2>
      <p>The page will reconnect automatically when new runtime events arrive.</p>
    </section>
  </section>
</section>
"#
}
