use axum::extract::Query;
use serde::Deserialize;

use crate::data::load_agents_page;
use crate::routes::{WebResult, internal_error};

use super::render::render_agents_page;

#[derive(Deserialize, Default)]
pub(crate) struct AgentQuery {
    pub(crate) agent: Option<String>,
}

pub(crate) async fn agents(Query(query): Query<AgentQuery>) -> WebResult {
    let page = load_agents_page(query.agent).map_err(internal_error)?;
    render_agents_page(page)
}
