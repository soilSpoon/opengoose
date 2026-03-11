use anyhow::Result;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, Sse};
use futures_core::Stream;
use std::convert::Infallible;
use std::time::Duration;

use crate::data::{SessionsPageView, load_sessions_page};
use crate::routes::pages::catalog_forms::SessionQuery;
use crate::routes::pages::catalog_templates::{
    SessionDetailTemplate, SessionsTemplate, matches_sessions_live_event,
    render_sessions_stream_html, sessions_stream_error_html,
};
use crate::routes::{BroadcastLiveOptions, WebResult, broadcast_live_sse};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct SessionsSpec;

impl CatalogPageSpec for SessionsSpec {
    type Page = SessionsPageView;
    type DetailTemplate = SessionDetailTemplate;
    type PageTemplate = SessionsTemplate;

    const TITLE: &'static str = "Sessions";
    const NAV: &'static str = "sessions";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_sessions_page(state.db.clone(), selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        SessionDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        SessionsTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn sessions(state: State<PageState>, query: Query<SessionQuery>) -> WebResult {
    render_catalog_handler::<SessionsSpec, SessionQuery>(state, query).await
}

pub(crate) async fn sessions_events(
    State(state): State<PageState>,
    Query(query): Query<SessionQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let db = state.db;
    let selected = query.session;
    let rx = state.event_bus.subscribe();
    let initial = render_sessions_stream_html(db.clone(), selected.clone())?;
    let render_db = db.clone();
    let render_selected = selected.clone();

    Ok(broadcast_live_sse(
        rx,
        initial,
        matches_sessions_live_event,
        move || render_sessions_stream_html(render_db.clone(), render_selected.clone()),
        BroadcastLiveOptions {
            keep_alive_text: "opengoose-sessions",
            fallback_interval: None::<Duration>,
            render_on_lagged: false,
            error_html: sessions_stream_error_html(),
        },
    ))
}
