use axum::extract::{Query, State};

use anyhow::Result;

use crate::data::{PluginStatusFilter, PluginsPageView, load_plugins_page_filtered};
use crate::routes::pages::catalog_forms::PluginQuery;
use crate::routes::pages::catalog_templates::{PluginDetailTemplate, PluginsTemplate};
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

use super::scaffold::{CatalogPageSpec, render_catalog_spec_page};

pub(in crate::routes::pages::catalog) struct PluginsSpec;

impl CatalogPageSpec for PluginsSpec {
    type Page = PluginsPageView;
    type DetailTemplate = PluginDetailTemplate;
    type PageTemplate = PluginsTemplate;

    const TITLE: &'static str = "Plugins";
    const NAV: &'static str = "plugins";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_plugins_page_filtered(state.db.clone(), selected, PluginStatusFilter::All)
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
    let page = load_plugins_page_filtered(
        state.db.clone(),
        query.plugin.clone(),
        PluginStatusFilter::from_query(query.status.as_deref()),
    )
    .map_err(internal_error)?;

    render_catalog_spec_page::<PluginsSpec>(page)
}
