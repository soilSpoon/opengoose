use anyhow::Result;
use axum::extract::{Query, State};

use crate::data::{QueuePageView, load_queue_page};
use crate::routes::WebResult;
use crate::routes::pages::catalog_forms::RunQuery;
use crate::routes::pages::catalog_templates::{QueueDetailTemplate, QueueTemplate};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct QueueSpec;

impl CatalogPageSpec for QueueSpec {
    type Page = QueuePageView;
    type DetailTemplate = QueueDetailTemplate;
    type PageTemplate = QueueTemplate;

    const TITLE: &'static str = "Queue";
    const NAV: &'static str = "queue";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_queue_page(state.db.clone(), selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        QueueDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        QueueTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn queue(state: State<PageState>, query: Query<RunQuery>) -> WebResult {
    render_catalog_handler::<QueueSpec, RunQuery>(state, query).await
}
