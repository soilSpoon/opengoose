use crate::error::TeamResult;

use super::types::{OrchestrationPattern, TeamDefinition};

impl TeamDefinition {
    /// Validate required fields and workflow-specific constraints.
    pub fn validate(&self) -> TeamResult<()> {
        if self.title.trim().is_empty() {
            return Err(opengoose_types::YamlStoreError::ValidationFailed(
                "title is required".into(),
            )
            .into());
        }
        if self.agents.is_empty() {
            return Err(opengoose_types::YamlStoreError::ValidationFailed(
                "at least one agent is required".into(),
            )
            .into());
        }
        for agent in &self.agents {
            if agent.profile.trim().is_empty() {
                return Err(opengoose_types::YamlStoreError::ValidationFailed(
                    "agent profile name cannot be empty".into(),
                )
                .into());
            }
        }
        if self.workflow == OrchestrationPattern::Router && self.router.is_none() {
            return Err(opengoose_types::YamlStoreError::ValidationFailed(
                "router workflow requires a `router` configuration".into(),
            )
            .into());
        }
        if self.workflow == OrchestrationPattern::FanOut && self.fan_out.is_none() {
            return Err(opengoose_types::YamlStoreError::ValidationFailed(
                "fan-out workflow requires a `fan_out` configuration".into(),
            )
            .into());
        }
        Ok(())
    }
}
