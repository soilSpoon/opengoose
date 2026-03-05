use serde::{Deserialize, Serialize};

use crate::error::{TeamError, TeamResult};

/// Orchestration pattern for a team (how agents are coordinated).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OrchestrationPattern {
    Chain,
    FanOut,
    Router,
}

/// A member agent within a team, referencing a profile by name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamAgent {
    /// Name of the agent profile to use.
    pub profile: String,
    /// Human-readable description of this agent's role in the team.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Router-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    pub strategy: RouterStrategy,
}

/// How the router picks an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RouterStrategy {
    ContentBased,
}

/// Fan-out-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanOutConfig {
    pub merge_strategy: MergeStrategy,
}

/// How fan-out results are merged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MergeStrategy {
    Concatenate,
    Summary,
}

/// A team definition — a YAML-serializable struct that composes agent profiles into a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDefinition {
    pub version: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub workflow: OrchestrationPattern,
    pub agents: Vec<TeamAgent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router: Option<RouterConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_out: Option<FanOutConfig>,
}

impl TeamDefinition {
    /// Team name (the title).
    pub fn name(&self) -> &str {
        &self.title
    }

    /// File-safe name: lowercase, spaces replaced with hyphens.
    pub fn file_name(&self) -> String {
        format!("{}.yaml", self.title.to_lowercase().replace(' ', "-"))
    }

    /// Parse from YAML string.
    pub fn from_yaml(yaml: &str) -> TeamResult<Self> {
        let team: Self = serde_yaml::from_str(yaml)?;
        team.validate()?;
        Ok(team)
    }

    /// Serialize to YAML string.
    pub fn to_yaml(&self) -> TeamResult<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// Validate required fields and workflow-specific constraints.
    pub fn validate(&self) -> TeamResult<()> {
        if self.title.trim().is_empty() {
            return Err(TeamError::ValidationFailed("title is required".into()));
        }
        if self.agents.is_empty() {
            return Err(TeamError::ValidationFailed(
                "at least one agent is required".into(),
            ));
        }
        for agent in &self.agents {
            if agent.profile.trim().is_empty() {
                return Err(TeamError::ValidationFailed(
                    "agent profile name cannot be empty".into(),
                ));
            }
        }
        if self.workflow == OrchestrationPattern::Router && self.router.is_none() {
            return Err(TeamError::ValidationFailed(
                "router workflow requires a `router` configuration".into(),
            ));
        }
        if self.workflow == OrchestrationPattern::FanOut && self.fan_out.is_none() {
            return Err(TeamError::ValidationFailed(
                "fan-out workflow requires a `fan_out` configuration".into(),
            ));
        }
        Ok(())
    }
}

impl opengoose_types::YamlDefinition for TeamDefinition {
    type Error = TeamError;

    fn title(&self) -> &str {
        &self.title
    }

    fn from_yaml(yaml: &str) -> TeamResult<Self> {
        TeamDefinition::from_yaml(yaml)
    }

    fn to_yaml(&self) -> TeamResult<String> {
        TeamDefinition::to_yaml(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_chain_yaml() {
        let yaml = include_str!("../teams/code-review.yaml");
        let team = TeamDefinition::from_yaml(yaml).unwrap();
        assert_eq!(team.name(), "code-review");
        assert_eq!(team.workflow, OrchestrationPattern::Chain);
        assert_eq!(team.agents.len(), 2);

        let serialized = team.to_yaml().unwrap();
        let reparsed = TeamDefinition::from_yaml(&serialized).unwrap();
        assert_eq!(reparsed.title, team.title);
    }

    #[test]
    fn round_trip_fan_out_yaml() {
        let yaml = include_str!("../teams/research-panel.yaml");
        let team = TeamDefinition::from_yaml(yaml).unwrap();
        assert_eq!(team.workflow, OrchestrationPattern::FanOut);
        assert!(team.fan_out.is_some());
    }

    #[test]
    fn round_trip_router_yaml() {
        let yaml = include_str!("../teams/smart-router.yaml");
        let team = TeamDefinition::from_yaml(yaml).unwrap();
        assert_eq!(team.workflow, OrchestrationPattern::Router);
        assert!(team.router.is_some());
    }

    #[test]
    fn validation_rejects_empty_agents() {
        let yaml = r#"
version: "1.0.0"
title: "empty"
workflow: chain
agents: []
"#;
        let err = TeamDefinition::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("at least one agent"));
    }

    #[test]
    fn validation_rejects_router_without_config() {
        let yaml = r#"
version: "1.0.0"
title: "bad-router"
workflow: router
agents:
  - profile: developer
"#;
        let err = TeamDefinition::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("router"));
    }
}
