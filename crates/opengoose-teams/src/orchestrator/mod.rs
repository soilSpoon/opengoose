mod delegation;
mod dispatch;
mod helpers;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use tokio::sync::Mutex;

use opengoose_profiles::ProfileStore;

use crate::runner::AgentRunner;
use crate::team::TeamDefinition;

pub(crate) use helpers::process_agent_communications;

/// Maximum delegation recursion depth to prevent infinite loops.
const MAX_DELEGATION_DEPTH: usize = 3;

/// Executes a team workflow by orchestrating multiple agent runners.
///
/// The internal agent pool is persistent: runners created for one message are
/// reused for subsequent messages in the same session, avoiding MCP extension
/// restarts between turns.
pub struct TeamOrchestrator {
    team: TeamDefinition,
    profile_store: ProfileStore,
    model_override: Option<String>,
    /// Per-session agent pool, keyed by agent profile name.
    /// Shared across `execute` and `resume` calls so extensions stay loaded.
    pool: Mutex<HashMap<String, AgentRunner>>,
}

impl TeamOrchestrator {
    pub fn new(team: TeamDefinition, profile_store: ProfileStore) -> Self {
        Self::new_with_model_override(team, profile_store, None)
    }

    pub fn new_with_model_override(
        team: TeamDefinition,
        profile_store: ProfileStore,
        model_override: Option<String>,
    ) -> Self {
        Self {
            team,
            profile_store,
            model_override,
            pool: Mutex::new(HashMap::new()),
        }
    }
}
