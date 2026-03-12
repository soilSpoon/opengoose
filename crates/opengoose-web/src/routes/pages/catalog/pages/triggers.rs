use anyhow::Result;
use axum::extract::{Query, State};

use crate::data::{TriggersPageView, load_triggers_page};
use crate::routes::WebResult;
use crate::routes::pages::catalog_forms::TriggerQuery;
use crate::routes::pages::catalog_templates::{TriggerDetailTemplate, TriggersTemplate};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct TriggersSpec;

impl CatalogPageSpec for TriggersSpec {
    type Page = TriggersPageView;
    type DetailTemplate = TriggerDetailTemplate;
    type PageTemplate = TriggersTemplate;

    const TITLE: &'static str = "Triggers";
    const NAV: &'static str = "triggers";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_triggers_page(state.db.clone(), selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        TriggerDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        TriggersTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn triggers(state: State<PageState>, query: Query<TriggerQuery>) -> WebResult {
    render_catalog_handler::<TriggersSpec, TriggerQuery>(state, query).await
}
