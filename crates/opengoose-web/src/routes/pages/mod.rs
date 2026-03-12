use axum::Router;
use axum::routing::get;

use crate::server::PageState;

mod api_keys;
mod catalog;
mod catalog_forms;
mod catalog_templates;
mod dashboard;
mod remote_agents;

pub use dashboard::render_dashboard_live_partial;

use api_keys::{api_key_action, api_keys};
use catalog::{
    agents, plugin_action, plugins, queue, runs, schedule_action, schedules, session_action,
    sessions, sessions_events, team_save, teams, trigger_action, trigger_workflow_action, triggers,
    workflows,
};
use dashboard::{dashboard, dashboard_events};
use remote_agents::{disconnect_remote_agent, remote_agents, remote_agents_events};

pub(crate) fn router(state: PageState) -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/dashboard/events", get(dashboard_events))
        .route("/sessions", get(sessions).post(session_action))
        .route("/sessions/events", get(sessions_events))
        .route("/runs", get(runs))
        .route("/plugins", get(plugins).post(plugin_action))
        .route("/agents", get(agents))
        .route("/api-keys", get(api_keys).post(api_key_action))
        .route("/remote-agents", get(remote_agents))
        .route("/remote-agents/events", get(remote_agents_events))
        .route(
            "/remote-agents/{name}/disconnect",
            axum::routing::delete(disconnect_remote_agent),
        )
        .route("/workflows", get(workflows))
        .route(
            "/workflows/{name}/trigger",
            axum::routing::post(trigger_workflow_action),
        )
        .route("/schedules", get(schedules).post(schedule_action))
        .route("/triggers", get(triggers).post(trigger_action))
        .route("/teams", get(teams).post(team_save))
        .route("/queue", get(queue))
        .with_state(state)
}

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use super::catalog::test_support::{
        render_plugin_detail, render_plugins_page, render_queue_detail, render_schedule_detail,
        render_schedules_page, render_session_detail, render_sessions_page, render_trigger_detail,
        render_triggers_page, render_workflow_detail, render_workflows_page,
    };
    pub(crate) use super::dashboard::test_support::render_dashboard_live;
}

#[cfg(test)]
mod tests;
