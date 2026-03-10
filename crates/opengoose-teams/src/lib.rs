mod chain_executor;
pub mod context;
mod defaults;
mod error;
mod executor_context;
mod fan_out_executor;
mod headless;
pub mod message_bus;
pub mod orchestrator;
pub mod plugin;
pub mod recipe_bridge;
pub mod remote;
mod router_executor;
pub mod runner;
pub mod scheduler;
mod store;
mod team;
pub mod triggers;

pub use context::OrchestrationContext;
pub use defaults::all_defaults;
pub use error::{TeamError, TeamResult};
pub use headless::{resume_headless, run_headless};
pub use message_bus::MessageBus;
pub use orchestrator::TeamOrchestrator;
pub use recipe_bridge::{profile_to_recipe, recipe_to_profile};
pub use remote::{ProtocolMessage, RemoteAgent, RemoteAgentRegistry, RemoteConfig};
pub use runner::{AgentOutput, AgentRunner};
pub use scheduler::run_due_schedules_once;
pub use store::TeamStore;
pub use team::{
    FanOutConfig, MergeStrategy, OrchestrationPattern, RouterConfig, RouterStrategy, TeamAgent,
    TeamDefinition,
};
pub use triggers::{
    spawn_event_bus_trigger_watcher, spawn_file_watch_trigger_watcher, spawn_trigger_watcher,
};
