use axum::extract::{Query, State};
use serde::Deserialize;

use crate::data::{load_queue_page, load_runs_page};
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

use super::render::{render_queue_page, render_runs_page};

#[derive(Deserialize, Default)]
pub(crate) struct RunQuery {
    pub(crate) run: Option<String>,
}

pub(crate) async fn runs(
    State(state): State<PageState>,
    Query(query): Query<RunQuery>,
) -> WebResult {
    let page = load_runs_page(state.db, query.run).map_err(internal_error)?;
    render_runs_page(page)
}

pub(crate) async fn queue(
    State(state): State<PageState>,
    Query(query): Query<RunQuery>,
) -> WebResult {
    let page = load_queue_page(state.db, query.run).map_err(internal_error)?;
    render_queue_page(page)
}
