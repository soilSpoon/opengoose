mod construction;
mod dispatch;
mod lifecycle;
pub(crate) mod output;
pub(crate) mod types;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use goose::agents::Agent;

pub use output::parse_agent_output;
pub use types::{AgentEventSummary, AgentOutput};

use types::ProviderTarget;

/// Wraps a Goose `Agent` for one-shot execution from an `AgentProfile`.
pub struct AgentRunner {
    agent: Arc<Agent>,
    session_id: String,
    profile_name: String,
    provider_chain: Vec<ProviderTarget>,
    max_turns: u32,
    retry_config: Option<goose::agents::RetryConfig>,
    /// The working directory for this runner's Goose session.
    cwd: PathBuf,
}
