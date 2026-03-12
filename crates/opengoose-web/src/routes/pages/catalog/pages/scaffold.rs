use anyhow::Result;
use askama::Template;
use axum::extract::{Query, State};

use crate::routes::pages::catalog_forms::{
    AgentQuery, PluginQuery, RunQuery, ScheduleQuery, SessionQuery, TeamQuery, TriggerQuery,
    WorkflowQuery,
};
use crate::routes::pages::catalog_templates::render_catalog_page;
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

pub(in crate::routes::pages::catalog::pages) trait CatalogSelection {
    fn selected(self) -> Option<String>;
}

impl CatalogSelection for SessionQuery {
    fn selected(self) -> Option<String> {
        self.session
    }
}

impl CatalogSelection for RunQuery {
    fn selected(self) -> Option<String> {
        self.run
    }
}

impl CatalogSelection for AgentQuery {
    fn selected(self) -> Option<String> {
        self.agent
    }
}

impl CatalogSelection for PluginQuery {
    fn selected(self) -> Option<String> {
        self.plugin
    }
}

impl CatalogSelection for TeamQuery {
    fn selected(self) -> Option<String> {
        self.team
    }
}

impl CatalogSelection for WorkflowQuery {
    fn selected(self) -> Option<String> {
        self.workflow
    }
}

impl CatalogSelection for ScheduleQuery {
    fn selected(self) -> Option<String> {
        self.schedule
    }
}

impl CatalogSelection for TriggerQuery {
    fn selected(self) -> Option<String> {
        self.trigger
    }
}

pub(in crate::routes::pages::catalog) trait CatalogPageSpec {
    type Page;
    type DetailTemplate: Template;
    type PageTemplate: Template;

    const TITLE: &'static str;
    const NAV: &'static str;

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page>;
    fn detail(page: &Self::Page) -> Self::DetailTemplate;
    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate;
}

fn render_catalog_spec<Spec>(state: &PageState, selected: Option<String>) -> WebResult
where
    Spec: CatalogPageSpec,
{
    let page = Spec::load(state, selected).map_err(internal_error)?;
    render_catalog_spec_page::<Spec>(page)
}

pub(in crate::routes::pages::catalog) fn render_catalog_spec_page<Spec>(
    page: Spec::Page,
) -> WebResult
where
    Spec: CatalogPageSpec,
{
    let detail = Spec::detail(&page);
    render_catalog_page(Spec::TITLE, Spec::NAV, page, &detail, Spec::page)
}

pub(in crate::routes::pages::catalog::pages) async fn render_catalog_handler<Spec, Q>(
    State(state): State<PageState>,
    Query(query): Query<Q>,
) -> WebResult
where
    Spec: CatalogPageSpec,
    Q: CatalogSelection,
{
    render_catalog_spec::<Spec>(&state, query.selected())
}
