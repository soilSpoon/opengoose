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

use crate::data::{DashboardView, load_dashboard};
use crate::routes::{
    PartialResult, WebResult, datastar_patch_elements_event, internal_error, render_partial,
    render_template,
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
    let db = state.db;
    let initial = render_dashboard_stream_html(db.clone())?;
    let event_stream = stream! {
        yield Ok(datastar_patch_elements_event(&initial));

        let mut ticker = tokio::time::interval(Duration::from_secs(4));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match render_dashboard_stream_html(db.clone()) {
                Ok(html) => yield Ok(datastar_patch_elements_event(&html)),
                Err(_) => {
                    yield Ok(datastar_patch_elements_event(dashboard_stream_error_html()));
                }
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-dashboard"),
    ))
}

fn render_dashboard_stream_html(db: Arc<Database>) -> PartialResult {
    let dashboard = load_dashboard(db).map_err(internal_error)?;
    let live_html = render_partial(&DashboardLiveTemplate { dashboard })?;
    Ok(format!(r#"<div id="dashboard-live">{live_html}</div>"#))
}

fn dashboard_stream_error_html() -> &'static str {
    r#"
<div id="dashboard-live">
  <section class="callout tone-danger">
    <p class="eyebrow">Stream degraded</p>
    <h2>Dashboard snapshot unavailable</h2>
    <p>The live board is retrying in the background. The rest of the page remains server-rendered and usable.</p>
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
