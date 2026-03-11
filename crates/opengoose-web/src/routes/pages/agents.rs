use askama::Template;
use axum::extract::Query;
use serde::Deserialize;

use super::{WebResult, internal_error, render_partial, render_template};
use crate::data::{AgentDetailView, AgentsPageView, load_agents_page};

#[derive(Deserialize, Default)]
pub(crate) struct AgentQuery {
    pub(crate) agent: Option<String>,
}

pub(crate) async fn agents(Query(query): Query<AgentQuery>) -> WebResult {
    let page = load_agents_page(query.agent).map_err(internal_error)?;
    let detail_html = render_partial(&AgentDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&AgentsTemplate {
        page_title: "Agents",
        current_nav: "agents",
        page,
        detail_html,
    })
}

#[derive(Template)]
#[template(path = "agents.html")]
struct AgentsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: AgentsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/agent_detail.html")]
struct AgentDetailTemplate {
    detail: AgentDetailView,
}
