use anyhow::Result;
use axum::extract::{Query, State};

use crate::data::{TeamsPageView, load_teams_page};
use crate::routes::WebResult;
use crate::routes::pages::catalog_forms::TeamQuery;
use crate::routes::pages::catalog_templates::{TeamEditorTemplate, TeamsTemplate};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct TeamsSpec;

impl CatalogPageSpec for TeamsSpec {
    type Page = TeamsPageView;
    type DetailTemplate = TeamEditorTemplate;
    type PageTemplate = TeamsTemplate;

    const TITLE: &'static str = "Teams";
    const NAV: &'static str = "teams";

    fn load(_state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_teams_page(selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        TeamEditorTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        TeamsTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn teams(state: State<PageState>, query: Query<TeamQuery>) -> WebResult {
    render_catalog_handler::<TeamsSpec, TeamQuery>(state, query).await
}
