use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, Sse};
use futures_core::Stream;
use opengoose_persistence::Database;
use opengoose_types::AppEventKind;

use crate::data::{DashboardView, load_dashboard};
use crate::routes::{
    BroadcastLiveOptions, PartialResult, WebResult, broadcast_live_sse, internal_error,
    render_partial, render_template,
};
use crate::server::PageState;

pub(crate) async fn dashboard(State(state): State<PageState>) -> WebResult {
    let dashboard = load_dashboard(state.db.clone()).map_err(internal_error)?;
    let live_html = render_partial(&DashboardLiveTemplate {
        dashboard: dashboard.clone(),
    })?;
    render_template(&DashboardTemplate {
        page_title: "OpenGoose Dashboard",
        current_nav: "dashboard",
        dashboard,
        live_html,
    })
}

pub(crate) async fn dashboard_events(
    State(state): State<PageState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let rx = state.event_bus.subscribe();
    let db = state.db;
    let initial = render_dashboard_stream_html(db.clone())?;
    let render_db = db.clone();

    Ok(broadcast_live_sse(
        rx,
        initial,
        matches_dashboard_live_event,
        move || render_dashboard_stream_html(render_db.clone()),
        BroadcastLiveOptions {
            keep_alive_text: "opengoose-dashboard",
            fallback_interval: Some(Duration::from_secs(30)),
            render_on_lagged: true,
            error_html: dashboard_stream_error_html(),
        },
    ))
}

fn render_dashboard_stream_html(db: Arc<Database>) -> PartialResult {
    let dashboard = load_dashboard(db).map_err(internal_error)?;
    let live_html = render_partial(&DashboardLiveTemplate { dashboard })?;
    Ok(format!(r#"<div id="dashboard-live">{live_html}</div>"#))
}

fn matches_dashboard_live_event(kind: &AppEventKind) -> bool {
    matches!(
        kind,
        AppEventKind::DashboardUpdated
            | AppEventKind::SessionUpdated { .. }
            | AppEventKind::MessageReceived { .. }
            | AppEventKind::ResponseSent { .. }
            | AppEventKind::PairingCompleted { .. }
            | AppEventKind::TeamActivated { .. }
            | AppEventKind::TeamDeactivated { .. }
            | AppEventKind::SessionDisconnected { .. }
            | AppEventKind::RunUpdated { .. }
            | AppEventKind::QueueUpdated { .. }
            | AppEventKind::StreamStarted { .. }
            | AppEventKind::StreamUpdated { .. }
            | AppEventKind::StreamCompleted { .. }
            | AppEventKind::TeamRunStarted { .. }
            | AppEventKind::TeamStepStarted { .. }
            | AppEventKind::TeamStepCompleted { .. }
            | AppEventKind::TeamStepFailed { .. }
            | AppEventKind::TeamRunCompleted { .. }
            | AppEventKind::TeamRunFailed { .. }
            | AppEventKind::ChannelReady { .. }
            | AppEventKind::ChannelDisconnected { .. }
            | AppEventKind::ChannelReconnecting { .. }
            | AppEventKind::AlertFired { .. }
    )
}

fn dashboard_stream_error_html() -> &'static str {
    r#"
<div id="dashboard-live">
  <section class="callout tone-danger">
    <p class="eyebrow">Stream degraded</p>
    <h2>Dashboard snapshot unavailable</h2>
    <p>The live board keeps listening for runtime events and falls back to a slower reconciliation sweep while the page stays usable.</p>
  </section>
</div>
"#
}

/// Render the dashboard live partial from a pre-built `DashboardView`.
///
/// Exposed for benchmarking. Returns the rendered HTML string or an error message.
pub fn render_dashboard_live_partial(dashboard: DashboardView) -> Result<String, String> {
    DashboardLiveTemplate { dashboard }
        .render()
        .map_err(|error| error.to_string())
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    dashboard: DashboardView,
    live_html: String,
}

#[derive(Template)]
#[template(path = "partials/dashboard_live.html")]
struct DashboardLiveTemplate {
    dashboard: DashboardView,
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn render_dashboard_live(dashboard: DashboardView) -> PartialResult {
        render_partial(&DashboardLiveTemplate { dashboard })
    }
}

#[cfg(test)]
mod tests {
    use opengoose_types::{Platform, SessionKey};

    use super::*;

    #[test]
    fn dashboard_live_event_matcher_includes_runtime_and_gateway_changes() {
        assert!(matches_dashboard_live_event(
            &AppEventKind::DashboardUpdated
        ));
        assert!(matches_dashboard_live_event(&AppEventKind::ChannelReady {
            platform: Platform::Discord,
        }));
        assert!(matches_dashboard_live_event(
            &AppEventKind::SessionUpdated {
                session_key: SessionKey::new(Platform::Discord, "ops", "chan-1"),
            }
        ));
        assert!(matches_dashboard_live_event(&AppEventKind::RunUpdated {
            team_run_id: "run-1".into(),
            status: "running".into(),
        }));
        assert!(!matches_dashboard_live_event(&AppEventKind::Error {
            context: "dashboard".into(),
            message: "boom".into(),
        }));
    }
}
