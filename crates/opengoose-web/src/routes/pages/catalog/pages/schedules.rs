use anyhow::Result;
use axum::extract::{Query, State};

use crate::data::{SchedulesPageView, load_schedules_page};
use crate::routes::WebResult;
use crate::routes::pages::catalog_forms::ScheduleQuery;
use crate::routes::pages::catalog_templates::{ScheduleDetailTemplate, SchedulesTemplate};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct SchedulesSpec;

impl CatalogPageSpec for SchedulesSpec {
    type Page = SchedulesPageView;
    type DetailTemplate = ScheduleDetailTemplate;
    type PageTemplate = SchedulesTemplate;

    const TITLE: &'static str = "Schedules";
    const NAV: &'static str = "schedules";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_schedules_page(state.db.clone(), selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        ScheduleDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        SchedulesTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn schedules(state: State<PageState>, query: Query<ScheduleQuery>) -> WebResult {
    render_catalog_handler::<SchedulesSpec, ScheduleQuery>(state, query).await
}
