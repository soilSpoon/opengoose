use anyhow::Result;
use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::sse::{Event, Sse};
use futures_core::Stream;
use std::convert::Infallible;
use std::time::Duration;

use crate::data::{
    AgentsPageView, QueuePageView, RunsPageView, SchedulesPageView, SessionsPageView,
    TeamsPageView, TriggersPageView, WorkflowsPageView, load_agents_page, load_queue_page,
    load_runs_page, load_schedules_page, load_sessions_page, load_teams_page, load_triggers_page,
    load_workflows_page,
};
use crate::routes::pages::catalog_forms::{
    AgentQuery, RunQuery, ScheduleQuery, SessionQuery, TeamQuery, TriggerQuery, WorkflowQuery,
};
use crate::routes::pages::catalog_templates::{
    AgentDetailTemplate, AgentsTemplate, QueueDetailTemplate, QueueTemplate, RunDetailTemplate,
    RunsTemplate, ScheduleDetailTemplate, SchedulesTemplate, SessionDetailTemplate,
    SessionsTemplate, TeamEditorTemplate, TeamsTemplate, TriggerDetailTemplate, TriggersTemplate,
    WorkflowDetailTemplate, WorkflowsTemplate, matches_sessions_live_event, render_catalog_page,
    render_sessions_stream_html, sessions_stream_error_html,
};
use crate::routes::{WebResult, broadcast_live_sse, internal_error};
use crate::server::PageState;

trait CatalogSelection {
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

async fn render_catalog_handler<Spec, Q>(
    State(state): State<PageState>,
    Query(query): Query<Q>,
) -> WebResult
where
    Spec: CatalogPageSpec,
    Q: CatalogSelection,
{
    render_catalog_spec::<Spec>(&state, query.selected())
}

pub(in crate::routes::pages::catalog) struct SessionsSpec;

impl CatalogPageSpec for SessionsSpec {
    type Page = SessionsPageView;
    type DetailTemplate = SessionDetailTemplate;
    type PageTemplate = SessionsTemplate;

    const TITLE: &'static str = "Sessions";
    const NAV: &'static str = "sessions";

    fn load(state: &PageState, selected: Option<String>) -> Result<Self::Page> {
        load_sessions_page(state.db.clone(), selected)
    }

    fn detail(page: &Self::Page) -> Self::DetailTemplate {
        SessionDetailTemplate {
            detail: page.selected.clone(),
        }
    }

    fn page(
        page_title: &'static str,
        current_nav: &'static str,
        page: Self::Page,
        detail_html: String,
    ) -> Self::PageTemplate {
        SessionsTemplate {
            page_title,
            current_nav,
            page,
            detail_html,
        }
    }
}

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

pub(crate) async fn sessions(state: State<PageState>, query: Query<SessionQuery>) -> WebResult {
    render_catalog_handler::<SessionsSpec, SessionQuery>(state, query).await
}

pub(crate) async fn sessions_events(
    State(state): State<PageState>,
    Query(query): Query<SessionQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, (StatusCode, Html<String>)> {
    let db = state.db;
    let selected = query.session;
    let rx = state.event_bus.subscribe();
    let initial = render_sessions_stream_html(db.clone(), selected.clone())?;
    let render_db = db.clone();
    let render_selected = selected.clone();

    Ok(broadcast_live_sse(
        rx,
        initial,
        "opengoose-sessions",
        None::<Duration>,
        false,
        matches_sessions_live_event,
        move || render_sessions_stream_html(render_db.clone(), render_selected.clone()),
        sessions_stream_error_html(),
    ))
}

pub(crate) async fn runs(state: State<PageState>, query: Query<RunQuery>) -> WebResult {
    render_catalog_handler::<RunsSpec, RunQuery>(state, query).await
}

pub(crate) async fn agents(state: State<PageState>, query: Query<AgentQuery>) -> WebResult {
    render_catalog_handler::<AgentsSpec, AgentQuery>(state, query).await
}

pub(crate) async fn workflows(state: State<PageState>, query: Query<WorkflowQuery>) -> WebResult {
    render_catalog_handler::<WorkflowsSpec, WorkflowQuery>(state, query).await
}

pub(crate) async fn schedules(state: State<PageState>, query: Query<ScheduleQuery>) -> WebResult {
    render_catalog_handler::<SchedulesSpec, ScheduleQuery>(state, query).await
}

pub(crate) async fn triggers(state: State<PageState>, query: Query<TriggerQuery>) -> WebResult {
    render_catalog_handler::<TriggersSpec, TriggerQuery>(state, query).await
}

pub(crate) async fn teams(state: State<PageState>, query: Query<TeamQuery>) -> WebResult {
    render_catalog_handler::<TeamsSpec, TeamQuery>(state, query).await
}

pub(crate) async fn queue(state: State<PageState>, query: Query<RunQuery>) -> WebResult {
    render_catalog_handler::<QueueSpec, RunQuery>(state, query).await
}
