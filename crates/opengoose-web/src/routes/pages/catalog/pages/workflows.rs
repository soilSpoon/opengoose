use anyhow::Result;
use axum::extract::{Query, State};

use crate::data::{WorkflowsPageView, load_workflows_page};
use crate::routes::WebResult;
use crate::routes::pages::catalog_forms::WorkflowQuery;
use crate::routes::pages::catalog_templates::{WorkflowDetailTemplate, WorkflowsTemplate};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct WorkflowsSpec;

impl CatalogPageSpec for WorkflowsSpec {
    type Page = WorkflowsPageView;
    type DetailTemplate = WorkflowDetailTemplate;
    type PageTemplate = WorkflowsTemplate;

    const TITLE: &'static str = "Workflows";
    const NAV: &'static str = "workflows";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_workflows_page(state.db.clone(), selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        WorkflowDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        WorkflowsTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn workflows(state: State<PageState>, query: Query<WorkflowQuery>) -> WebResult {
    render_catalog_handler::<WorkflowsSpec, WorkflowQuery>(state, query).await
}
