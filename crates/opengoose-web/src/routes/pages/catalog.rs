mod actions;
mod pages;

pub(crate) use actions::{
    schedule_action, session_action, team_save, trigger_action, trigger_workflow_action,
};
pub(crate) use pages::{
    agents, queue, runs, schedules, sessions, sessions_events, teams, triggers, workflows,
};

#[cfg(test)]
pub(crate) mod test_support {
    use crate::data::{
        QueueDetailView, ScheduleEditorView, SchedulesPageView, SessionDetailView,
        SessionsPageView, TriggerDetailView, TriggersPageView, WorkflowDetailView,
        WorkflowsPageView,
    };
    use crate::routes::pages::catalog_templates::{
        QueueDetailTemplate, ScheduleDetailTemplate, SchedulesTemplate, SessionDetailTemplate,
        SessionsTemplate, TriggerDetailTemplate, TriggersTemplate, WorkflowDetailTemplate,
        WorkflowsTemplate,
    };
    use crate::routes::{PartialResult, render_partial};

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

    pub(crate) fn render_trigger_detail(detail: TriggerDetailView) -> PartialResult {
        render_partial(&TriggerDetailTemplate { detail })
    }

    pub(crate) fn render_triggers_page(
        page: TriggersPageView,
        detail_html: String,
    ) -> PartialResult {
        render_partial(&TriggersTemplate {
            page_title: "Triggers",
            current_nav: "triggers",
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
