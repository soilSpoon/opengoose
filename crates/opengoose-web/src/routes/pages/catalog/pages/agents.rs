use anyhow::Result;
use axum::extract::{Query, State};

use crate::data::{AgentsPageView, load_agents_page};
use crate::routes::WebResult;
use crate::routes::pages::catalog_forms::AgentQuery;
use crate::routes::pages::catalog_templates::{AgentDetailTemplate, AgentsTemplate};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct AgentsSpec;

impl CatalogPageSpec for AgentsSpec {
    type Page = AgentsPageView;
    type DetailTemplate = AgentDetailTemplate;
    type PageTemplate = AgentsTemplate;

    const TITLE: &'static str = "Agents";
    const NAV: &'static str = "agents";

    fn load(_state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_agents_page(selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        AgentDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        AgentsTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn agents(state: State<PageState>, query: Query<AgentQuery>) -> WebResult {
    render_catalog_handler::<AgentsSpec, AgentQuery>(state, query).await
}
