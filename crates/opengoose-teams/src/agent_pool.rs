use std::collections::HashMap;

use anyhow::Result;

use opengoose_profiles::AgentProfile;

use crate::runner::AgentRunner;

/// Caches `AgentRunner` instances by profile name within a single
/// orchestration run, avoiding repeated calls to `AgentRunner::from_profile`
/// which each generate a new UUID, set up provider, and process extensions.
pub struct AgentPool {
    runners: HashMap<String, AgentRunner>,
}

impl AgentPool {
    pub fn new() -> Self {
        Self {
            runners: HashMap::new(),
        }
    }

    /// Get or create an `AgentRunner` for the given profile.
    ///
    /// If a runner for this profile name already exists in the pool, it is
    /// returned. Otherwise a new one is created, cached, and returned.
    pub async fn get_or_create(&mut self, profile: &AgentProfile) -> Result<&AgentRunner> {
        let name = profile.name().to_string();
        if !self.runners.contains_key(&name) {
            let runner = AgentRunner::from_profile(profile).await?;
            self.runners.insert(name.clone(), runner);
        }
        Ok(self.runners.get(&name).unwrap())
    }

    /// Create a runner for use in a spawned task (returns owned runner).
    ///
    /// For fan-out execution where runners must be moved into async tasks,
    /// we cannot return references. This method creates a fresh runner
    /// but does NOT cache it, since the task will own it.
    pub async fn create_for_task(profile: &AgentProfile) -> Result<AgentRunner> {
        AgentRunner::from_profile(profile).await
    }
}
