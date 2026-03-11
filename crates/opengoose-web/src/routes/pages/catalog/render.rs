use askama::Template;
use std::sync::Arc;

use opengoose_persistence::Database;

use crate::data::{
    AgentDetailView, AgentsPageView, QueueDetailView, QueuePageView, RunDetailView, RunsPageView,
    ScheduleEditorView, SchedulesPageView, SessionDetailView, SessionsPageView, TeamEditorView,
    TeamsPageView, TriggerDetailView, TriggersPageView, WorkflowDetailView, WorkflowsPageView,
    load_sessions_page,
};
use crate::routes::{PartialResult, WebResult, internal_error, render_partial, render_template};

pub(super) fn render_sessions_page(page: SessionsPageView) -> WebResult {
    let detail_html = render_partial(&SessionDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&SessionsTemplate {
        page_title: "Sessions",
        current_nav: "sessions",
        page,
        detail_html,
    })
}

pub(super) fn render_runs_page(page: RunsPageView) -> WebResult {
    let detail_html = render_partial(&RunDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&RunsTemplate {
        page_title: "Runs",
        current_nav: "runs",
        page,
        detail_html,
    })
}

pub(super) fn render_agents_page(page: AgentsPageView) -> WebResult {
    let detail_html = render_partial(&AgentDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&AgentsTemplate {
        page_title: "Agents",
        current_nav: "agents",
        page,
        detail_html,
    })
}

pub(super) fn render_workflows_page(page: WorkflowsPageView) -> WebResult {
    let detail_html = render_partial(&WorkflowDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&WorkflowsTemplate {
        page_title: "Workflows",
        current_nav: "workflows",
        page,
        detail_html,
    })
}

pub(super) fn render_schedules_page(page: SchedulesPageView) -> WebResult {
    let detail_html = render_partial(&ScheduleDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&SchedulesTemplate {
        page_title: "Schedules",
        current_nav: "schedules",
        page,
        detail_html,
    })
}

pub(super) fn render_triggers_page(page: TriggersPageView) -> WebResult {
    let detail_html = render_partial(&TriggerDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&TriggersTemplate {
        page_title: "Triggers",
        current_nav: "triggers",
        page,
        detail_html,
    })
}

pub(super) fn render_teams_page(page: TeamsPageView) -> WebResult {
    let detail_html = render_partial(&TeamEditorTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&TeamsTemplate {
        page_title: "Teams",
        current_nav: "teams",
        page,
        detail_html,
    })
}

pub(super) fn render_queue_page(page: QueuePageView) -> WebResult {
    let detail_html = render_partial(&QueueDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&QueueTemplate {
        page_title: "Queue",
        current_nav: "queue",
        page,
        detail_html,
    })
}

pub(super) fn render_sessions_stream_html(
    db: Arc<Database>,
    selected: Option<String>,
) -> PartialResult {
    let page = load_sessions_page(db, selected).map_err(internal_error)?;
    let detail_html = render_partial(&SessionDetailTemplate {
        detail: page.selected.clone(),
    })?;
    let intro_html = render_partial(&SessionsPageIntroTemplate { page: page.clone() })?;
    let shell_html = render_partial(&SessionsShellTemplate { page, detail_html })?;
    Ok(format!("{intro_html}{shell_html}"))
}

pub(super) fn render_workflow_trigger_status(message: String, tone: &'static str) -> PartialResult {
    render_partial(&WorkflowTriggerStatusTemplate { message, tone })
}

#[derive(Template)]
#[template(path = "sessions.html")]
struct SessionsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: SessionsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/session_detail.html")]
struct SessionDetailTemplate {
    detail: SessionDetailView,
}

#[derive(Template)]
#[template(path = "partials/sessions_page_intro.html")]
struct SessionsPageIntroTemplate {
    page: SessionsPageView,
}

#[derive(Template)]
#[template(path = "partials/sessions_shell.html")]
struct SessionsShellTemplate {
    page: SessionsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "runs.html")]
struct RunsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: RunsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/run_detail.html")]
struct RunDetailTemplate {
    detail: RunDetailView,
}

#[derive(Template)]
#[template(path = "agents.html")]
struct AgentsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: AgentsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/agent_detail.html")]
struct AgentDetailTemplate {
    detail: AgentDetailView,
}

#[derive(Template)]
#[template(path = "workflows.html")]
struct WorkflowsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: WorkflowsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/workflow_detail.html")]
struct WorkflowDetailTemplate {
    detail: WorkflowDetailView,
}

#[derive(Template)]
#[template(path = "partials/workflow_trigger_status.html")]
struct WorkflowTriggerStatusTemplate {
    message: String,
    tone: &'static str,
}

#[derive(Template)]
#[template(path = "schedules.html")]
struct SchedulesTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: SchedulesPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/schedule_detail.html")]
struct ScheduleDetailTemplate {
    detail: ScheduleEditorView,
}

#[derive(Template)]
#[template(path = "triggers.html")]
struct TriggersTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: TriggersPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/trigger_detail.html")]
struct TriggerDetailTemplate {
    detail: TriggerDetailView,
}

#[derive(Template)]
#[template(path = "teams.html")]
struct TeamsTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: TeamsPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/team_editor.html")]
struct TeamEditorTemplate {
    detail: TeamEditorView,
}

#[derive(Template)]
#[template(path = "queue.html")]
struct QueueTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: QueuePageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/queue_detail.html")]
struct QueueDetailTemplate {
    detail: QueueDetailView,
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::data::{
        QueueDetailView, ScheduleEditorView, SchedulesPageView, SessionDetailView,
        SessionsPageView, WorkflowDetailView, WorkflowsPageView,
    };
    use crate::routes::{PartialResult, render_partial};

    use super::{
        QueueDetailTemplate, ScheduleDetailTemplate, SchedulesTemplate, SessionDetailTemplate,
        SessionsTemplate, WorkflowDetailTemplate, WorkflowsTemplate,
    };

    pub(crate) fn render_session_detail(detail: SessionDetailView) -> PartialResult {
        render_partial(&SessionDetailTemplate { detail })
    }

    pub(crate) fn render_sessions_page(
        page: SessionsPageView,
        detail_html: String,
    ) -> PartialResult {
        render_partial(&SessionsTemplate {
            page_title: "Sessions",
            current_nav: "sessions",
            page,
            detail_html,
        })
    }

    pub(crate) fn render_queue_detail(detail: QueueDetailView) -> PartialResult {
        render_partial(&QueueDetailTemplate { detail })
    }

    pub(crate) fn render_schedule_detail(detail: ScheduleEditorView) -> PartialResult {
        render_partial(&ScheduleDetailTemplate { detail })
    }

    pub(crate) fn render_schedules_page(
        page: SchedulesPageView,
        detail_html: String,
    ) -> PartialResult {
        render_partial(&SchedulesTemplate {
            page_title: "Schedules",
            current_nav: "schedules",
            page,
            detail_html,
        })
    }

    pub(crate) fn render_workflow_detail(detail: WorkflowDetailView) -> PartialResult {
        render_partial(&WorkflowDetailTemplate { detail })
    }

    pub(crate) fn render_workflows_page(
        page: WorkflowsPageView,
        detail_html: String,
    ) -> PartialResult {
        render_partial(&WorkflowsTemplate {
            page_title: "Workflows",
            current_nav: "workflows",
            page,
            detail_html,
        })
    }
}
