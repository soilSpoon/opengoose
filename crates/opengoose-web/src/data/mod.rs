mod agents;
mod dashboard;
mod plugins;
mod queue;
mod remote_agents;
mod runs;
mod schedules;
mod sessions;
mod status;
mod teams;
mod triggers;
mod utils;
mod views;
mod workflows;

pub use agents::load_agents_page;
pub use dashboard::load_dashboard;
pub use opengoose_types::HealthResponse;
pub use plugins::{
    PluginInstallInput, delete_plugin, install_plugin_from_path, load_plugins_page,
    toggle_plugin_state,
};
pub use queue::load_queue_page;
pub use remote_agents::load_remote_agents_page;
pub use runs::load_runs_page;
pub use schedules::{
    ScheduleSaveInput, delete_schedule, load_schedules_page, save_schedule, toggle_schedule,
};
pub use sessions::load_sessions_page;
pub use status::{load_status_page, probe_health, probe_readiness};
pub use teams::{load_teams_page, save_team_yaml};
pub use triggers::load_triggers_page;
pub use views::*;
pub use workflows::load_workflows_page;
