pub mod context;
mod defaults;
mod error;
pub mod orchestrator;
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
