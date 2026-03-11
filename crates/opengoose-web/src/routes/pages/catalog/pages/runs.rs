use anyhow::Result;
use axum::extract::{Query, State};

use crate::data::{RunsPageView, load_runs_page};
use crate::routes::WebResult;
use crate::routes::pages::catalog_forms::RunQuery;
use crate::routes::pages::catalog_templates::{RunDetailTemplate, RunsTemplate};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct RunsSpec;

impl CatalogPageSpec for RunsSpec {
    type Page = RunsPageView;
    type DetailTemplate = RunDetailTemplate;
    type PageTemplate = RunsTemplate;

    const TITLE: &'static str = "Runs";
    const NAV: &'static str = "runs";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_runs_page(state.db.clone(), selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        RunDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        RunsTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn runs(state: State<PageState>, query: Query<RunQuery>) -> WebResult {
    render_catalog_handler::<RunsSpec, RunQuery>(state, query).await
}
