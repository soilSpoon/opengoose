mod agents;
mod render;
mod runs;
mod schedules;
mod sessions;
mod teams;
mod triggers;
mod workflows;

pub(crate) use agents::agents;
pub(crate) use runs::{queue, runs};
pub(crate) use schedules::{schedule_action, schedules};
pub(crate) use sessions::{sessions, sessions_events};
pub(crate) use teams::{team_save, teams};
pub(crate) use triggers::{trigger_action, triggers};
pub(crate) use workflows::{trigger_workflow_action, workflows};

#[cfg(test)]
pub(crate) use agents::AgentQuery;
#[cfg(test)]
pub(crate) use runs::RunQuery;
#[cfg(test)]
pub(crate) use schedules::{ScheduleActionForm, ScheduleQuery};
#[cfg(test)]
pub(crate) use sessions::SessionQuery;
#[cfg(test)]
pub(crate) use teams::TeamSaveForm;
#[cfg(test)]
pub(crate) use triggers::{TriggerActionForm, TriggerQuery};
#[cfg(test)]
pub(crate) use workflows::WorkflowQuery;

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use super::render::test_support::{
        render_queue_detail, render_schedule_detail, render_schedules_page, render_session_detail,
        render_sessions_page, render_workflow_detail, render_workflows_page,
    };
}
