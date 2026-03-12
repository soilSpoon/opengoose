use askama::Template;
use opengoose_types::AppEventKind;
use std::sync::Arc;

use crate::data::{
    AgentDetailView, AgentsPageView, PluginDetailView, PluginsPageView, QueueDetailView,
    QueuePageView, RunDetailView, RunsPageView, ScheduleEditorView, SchedulesPageView,
    SessionDetailView, SessionsPageView, TeamEditorView, TeamsPageView, TriggerDetailView,
    TriggersPageView, WorkflowDetailView, WorkflowsPageView, load_sessions_page,
};
use crate::routes::{PartialResult, WebResult, internal_error, render_partial, render_template};

macro_rules! catalog_page_template {
    ($name:ident, $path:literal, $page_ty:ty) => {
        #[derive(Template)]
        #[template(path = $path)]
        pub(super) struct $name {
            pub(super) page_title: &'static str,
            pub(super) current_nav: &'static str,
            pub(super) page: $page_ty,
            pub(super) detail_html: String,
        }
    };
}

macro_rules! detail_template {
    ($name:ident, $path:literal, $detail_ty:ty) => {
        #[derive(Template)]
        #[template(path = $path)]
        pub(super) struct $name {
            pub(super) detail: $detail_ty,
        }
    };
}

pub(super) fn render_catalog_page<Page, DetailTemplate, PageTemplate, F>(
    page_title: &'static str,
    current_nav: &'static str,
    page: Page,
    detail_template: &DetailTemplate,
    page_template: F,
) -> WebResult
where
    DetailTemplate: Template,
    PageTemplate: Template,
    F: FnOnce(&'static str, &'static str, Page, String) -> PageTemplate,
{
    let detail_html = render_partial(detail_template)?;
    render_template(&page_template(page_title, current_nav, page, detail_html))
}

catalog_page_template!(SessionsTemplate, "sessions.html", SessionsPageView);
detail_template!(
    SessionDetailTemplate,
    "partials/session_detail.html",
    SessionDetailView
);

#[derive(Template)]
#[template(path = "partials/sessions_live.html")]
pub(super) struct SessionsLiveTemplate {
    pub(super) page: SessionsPageView,
    pub(super) detail_html: String,
}

catalog_page_template!(RunsTemplate, "runs.html", RunsPageView);
detail_template!(RunDetailTemplate, "partials/run_detail.html", RunDetailView);

catalog_page_template!(AgentsTemplate, "agents.html", AgentsPageView);
detail_template!(
    AgentDetailTemplate,
    "partials/agent_detail.html",
    AgentDetailView
);

catalog_page_template!(PluginsTemplate, "plugins.html", PluginsPageView);
detail_template!(
    PluginDetailTemplate,
    "partials/plugin_detail.html",
    PluginDetailView
);

catalog_page_template!(WorkflowsTemplate, "workflows.html", WorkflowsPageView);
detail_template!(
    WorkflowDetailTemplate,
    "partials/workflow_detail.html",
    WorkflowDetailView
);

#[derive(Template)]
#[template(path = "partials/workflow_trigger_status.html")]
pub(super) struct WorkflowTriggerStatusTemplate {
    pub(super) message: String,
    pub(super) tone: &'static str,
}

catalog_page_template!(SchedulesTemplate, "schedules.html", SchedulesPageView);
detail_template!(
    ScheduleDetailTemplate,
    "partials/schedule_detail.html",
    ScheduleEditorView
);

catalog_page_template!(TriggersTemplate, "triggers.html", TriggersPageView);
detail_template!(
    TriggerDetailTemplate,
    "partials/trigger_detail.html",
    TriggerDetailView
);

catalog_page_template!(TeamsTemplate, "teams.html", TeamsPageView);
detail_template!(
    TeamEditorTemplate,
    "partials/team_editor.html",
    TeamEditorView
);

catalog_page_template!(QueueTemplate, "queue.html", QueuePageView);
detail_template!(
    QueueDetailTemplate,
    "partials/queue_detail.html",
    QueueDetailView
);

pub(super) fn render_sessions_stream_html(
    db: Arc<opengoose_persistence::Database>,
    selected: Option<String>,
) -> PartialResult {
    let page = load_sessions_page(db, selected).map_err(internal_error)?;
    let detail_html = render_partial(&SessionDetailTemplate {
        detail: page.selected.clone(),
    })?;
    render_partial(&SessionsLiveTemplate { page, detail_html })
}

pub(super) fn render_workflow_trigger_status(message: String, tone: &'static str) -> PartialResult {
    render_partial(&WorkflowTriggerStatusTemplate { message, tone })
}

pub(super) fn matches_sessions_live_event(kind: &AppEventKind) -> bool {
    matches!(
        kind,
        AppEventKind::SessionUpdated { .. }
            | AppEventKind::MessageReceived { .. }
            | AppEventKind::ResponseSent { .. }
            | AppEventKind::PairingCompleted { .. }
            | AppEventKind::TeamActivated { .. }
            | AppEventKind::TeamDeactivated { .. }
            | AppEventKind::SessionDisconnected { .. }
            | AppEventKind::StreamStarted { .. }
            | AppEventKind::StreamUpdated { .. }
            | AppEventKind::StreamCompleted { .. }
            | AppEventKind::RunUpdated { .. }
            | AppEventKind::TeamRunStarted { .. }
            | AppEventKind::TeamStepStarted { .. }
            | AppEventKind::TeamStepCompleted { .. }
            | AppEventKind::TeamStepFailed { .. }
            | AppEventKind::TeamRunCompleted { .. }
            | AppEventKind::TeamRunFailed { .. }
    )
}

pub(super) fn sessions_stream_error_html() -> &'static str {
    r#"
<section id="detail-shell" class="detail-shell">
  <section class="detail-frame">
    <section class="callout tone-danger">
      <p class="eyebrow">Session stream degraded</p>
      <h2>Live session updates paused</h2>
      <p>The page will reconnect automatically when new runtime events arrive.</p>
    </section>
  </section>
</section>
"#
}
