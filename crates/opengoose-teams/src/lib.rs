//! Multi-agent team orchestration for OpenGoose.
//!
//! Provides the plumbing for running multiple Goose agents as a coordinated
//! team: fan-out execution, chain-of-responsibility routing, a shared message
//! bus ([`message_bus`]), a trigger/scheduler system ([`triggers`],
//! [`scheduler`]), and remote-agent integration ([`remote`]).
//!
//! The primary entry point for the core engine is [`TeamOrchestrator`].

mod chain_executor;
pub mod context;
mod defaults;
mod error;
mod executor_context;
mod fan_out_executor;
mod headless;
pub mod landing;
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
pub mod witness;
pub mod triggers;

pub use context::OrchestrationContext;
pub use defaults::all_defaults;
pub use error::{TeamError, TeamResult};
pub use headless::{
    resume_headless, run_headless, run_headless_with_model, run_headless_with_project,
};
pub use message_bus::MessageBus;
pub use opengoose_projects::{ProjectContext, ProjectDefinition, ProjectStore};
pub use orchestrator::TeamOrchestrator;
pub use recipe_bridge::{profile_to_recipe, recipe_to_profile};
pub use remote::{ProtocolMessage, RemoteAgent, RemoteAgentRegistry, RemoteConfig};
pub use runner::{AgentEventSummary, AgentOutput, AgentRunner};
pub use scheduler::run_due_schedules_once;
pub use store::TeamStore;
pub use team::{
    CommunicationMode, FanOutConfig, MergeStrategy, OrchestrationPattern, RouterConfig,
    RouterStrategy, TeamAgent, TeamDefinition,
};
pub use triggers::{
    spawn_event_bus_trigger_watcher, spawn_file_watch_trigger_watcher, spawn_trigger_watcher,
};
pub use landing::LandingReport;
pub use witness::{AgentState, AgentStatus, WitnessConfig, WitnessHandle, spawn_witness};
