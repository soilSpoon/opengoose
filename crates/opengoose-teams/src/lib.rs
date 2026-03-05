mod agent_pool;
mod chain_executor;
pub mod context;
mod defaults;
mod error;
mod fan_out_executor;
pub mod orchestrator;
mod prompt_context;
mod router_executor;
pub mod runner;
mod store;
mod team;

pub use context::OrchestrationContext;
pub use defaults::all_defaults;
pub use error::{TeamError, TeamResult};
pub use orchestrator::TeamOrchestrator;
pub use runner::{AgentOutput, AgentRunner};
pub use store::TeamStore;
pub use team::{
    FanOutConfig, MergeStrategy, OrchestrationPattern, RouterConfig, RouterStrategy, TeamAgent,
    TeamDefinition,
};
