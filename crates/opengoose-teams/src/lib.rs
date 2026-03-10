mod chain_executor;
pub mod context;
mod defaults;
mod error;
mod executor_context;
mod fan_out_executor;
mod headless;
pub mod orchestrator;
pub mod recipe_bridge;
mod router_executor;
pub mod runner;
mod store;
mod team;

pub use context::OrchestrationContext;
pub use defaults::all_defaults;
pub use error::{TeamError, TeamResult};
pub use headless::{resume_headless, run_headless};
pub use orchestrator::TeamOrchestrator;
pub use recipe_bridge::{profile_to_recipe, recipe_to_profile};
pub use runner::{AgentOutput, AgentRunner};
pub use store::TeamStore;
pub use team::{
    FanOutConfig, MergeStrategy, OrchestrationPattern, RouterConfig, RouterStrategy, TeamAgent,
    TeamDefinition,
};
