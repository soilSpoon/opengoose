use anyhow::Result;
use axum::extract::{Query, State};

use crate::data::{PluginsPageView, load_plugins_page};
use crate::routes::WebResult;
use crate::routes::pages::catalog_forms::PluginQuery;
use crate::routes::pages::catalog_templates::{PluginDetailTemplate, PluginsTemplate};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_handler};

pub(in crate::routes::pages::catalog) struct PluginsSpec;

impl CatalogPageSpec for PluginsSpec {
    type Page = PluginsPageView;
    type DetailTemplate = PluginDetailTemplate;
    type PageTemplate = PluginsTemplate;

    const TITLE: &'static str = "Plugins";
    const NAV: &'static str = "plugins";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_plugins_page(state.db.clone(), selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        PluginDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        PluginsTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

pub(crate) async fn plugins(state: State<PageState>, query: Query<PluginQuery>) -> WebResult {
    render_catalog_handler::<PluginsSpec, PluginQuery>(state, query).await
}
