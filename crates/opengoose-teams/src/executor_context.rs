use std::collections::HashMap;

use opengoose_profiles::ProfileStore;

use crate::error::TeamError;
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
) -> Result<opengoose_profiles::AgentProfile, TeamError> {
    match store.get(name) {
        Ok(profile) => Ok(profile),
        Err(opengoose_profiles::ProfileError::NotFound(_)) => {
            Err(TeamError::ProfileNotFound(name.to_string()))
        }
        Err(opengoose_profiles::ProfileError::Store(err)) => Err(TeamError::Store(err)),
        Err(err) => Err(TeamError::AgentFailed(format!(
            "failed to resolve profile `{name}`: {err}"
        ))),
    }
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

    #[test]
    fn resolve_profile_preserves_store_failures() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let store = ProfileStore::with_dir(tmp.path().to_path_buf());
        let err = resolve_profile(&store, "ghost").unwrap_err();
        assert!(matches!(err, TeamError::Store(_)));
    }
}
