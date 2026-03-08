use std::collections::HashMap;

use anyhow::anyhow;

use opengoose_profiles::ProfileStore;

use crate::runner::AgentRunner;
use crate::team::TeamDefinition;

/// Shared fields for all executor types (Chain, FanOut, Router).
///
/// Extracting these into a single struct eliminates identical struct
/// definitions and constructors that were previously duplicated across
/// each executor.
pub(crate) struct ExecutorContext<'a> {
    pub team: &'a TeamDefinition,
    pub profile_store: &'a ProfileStore,
    pub pool: &'a mut HashMap<String, AgentRunner>,
}

impl<'a> ExecutorContext<'a> {
    pub fn new(
        team: &'a TeamDefinition,
        profile_store: &'a ProfileStore,
        pool: &'a mut HashMap<String, AgentRunner>,
    ) -> Self {
        Self {
            team,
            profile_store,
            pool,
        }
    }
}

/// Look up an agent profile by name, returning a consistent error message
/// when the profile is not found.
pub(crate) fn resolve_profile(
    store: &ProfileStore,
    name: &str,
) -> anyhow::Result<opengoose_profiles::AgentProfile> {
    store
        .get(name)
        .map_err(|_| anyhow!("profile `{name}` not found"))
}

/// Inject the agent's team role into the runner's system prompt.
pub(crate) async fn inject_team_role(runner: &AgentRunner, role: &str) {
    runner
        .extend_system_prompt("team_role", &format!("Your role: {role}"))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_profile_returns_error_for_missing() {
        let store = ProfileStore::with_dir(std::path::PathBuf::from("/tmp/nonexistent-profiles"));
        let err = resolve_profile(&store, "ghost").unwrap_err();
        assert!(err.to_string().contains("profile `ghost` not found"));
    }
}
