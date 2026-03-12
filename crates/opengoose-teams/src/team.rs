use serde::{Deserialize, Serialize};

use goose::agents::extension::ExtensionConfig;
use goose::recipe::{Recipe, SubRecipe};
use opengoose_profiles::ProfileStore;

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

/// How agents communicate within a team.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CommunicationMode {
    /// Agents communicate via @mention and [BROADCAST] text parsing.
    #[default]
    TextParsing,
    /// Agents communicate via MCP team-tools extension (structured JSON-RPC).
    McpTools,
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
    /// How agents communicate. Defaults to text-parsing (@mention / [BROADCAST]).
    #[serde(default, skip_serializing_if = "is_default_comm_mode")]
    pub communication_mode: CommunicationMode,
}

fn is_default_comm_mode(mode: &CommunicationMode) -> bool {
    *mode == CommunicationMode::TextParsing
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

    /// Convert this team into a Goose `Recipe` for Goose CLI compatibility.
    ///
    /// Each team member becomes a sub-recipe (Summon extension), and the
    /// orchestration logic is described in the recipe instructions. This
    /// allows the team to be executed via `goose run --recipe team.yaml`.
    pub fn to_recipe(&self, profile_store: &ProfileStore) -> Recipe {
        let sub_recipes: Vec<SubRecipe> = self
            .agents
            .iter()
            .map(|a| SubRecipe {
                name: a.profile.clone(),
                path: profile_store.profile_path(&a.profile),
                values: None,
                sequential_when_repeated: matches!(self.workflow, OrchestrationPattern::Chain),
                description: a.role.clone(),
            })
            .collect();

        let instructions = self.generate_orchestration_instructions();

        Recipe {
            version: self.version.clone(),
            title: self.title.clone(),
            description: self.description.clone().unwrap_or_default(),
            instructions: Some(instructions),
            prompt: None,
            extensions: Some(vec![ExtensionConfig::Platform {
                name: "summon".into(),
                description: String::new(),
                display_name: None,
                bundled: None,
                available_tools: vec![],
            }]),
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            sub_recipes: Some(sub_recipes),
            retry: None,
        }
    }

    fn generate_orchestration_instructions(&self) -> String {
        match self.workflow {
            OrchestrationPattern::Chain => {
                let steps: Vec<String> = self
                    .agents
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        format!(
                            "{}. Delegate to '{}'{}",
                            i + 1,
                            a.profile,
                            a.role
                                .as_ref()
                                .map(|r| format!(" ({r})"))
                                .unwrap_or_default()
                        )
                    })
                    .collect();
                format!(
                    "Execute the following agents in sequence, \
                     passing each output as input to the next:\n{}",
                    steps.join("\n")
                )
            }
            OrchestrationPattern::FanOut => {
                let agents: Vec<String> = self
                    .agents
                    .iter()
                    .map(|a| format!("- '{}'", a.profile))
                    .collect();
                format!(
                    "Delegate to ALL of the following agents simultaneously (async), \
                     then synthesize their results:\n{}",
                    agents.join("\n")
                )
            }
            OrchestrationPattern::Router => {
                let agents: Vec<String> = self
                    .agents
                    .iter()
                    .map(|a| {
                        format!(
                            "- '{}'{}",
                            a.profile,
                            a.role
                                .as_ref()
                                .map(|r| format!(": {r}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect();
                format!(
                    "Analyze the input and delegate to the most appropriate agent:\n{}",
                    agents.join("\n")
                )
            }
        }
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

    #[test]
    fn to_recipe_chain() {
        let (_tmp, store) = temp_store_with_defaults();
        let yaml = include_str!("../teams/code-review.yaml");
        let team = TeamDefinition::from_yaml(yaml).unwrap();

        let recipe = team.to_recipe(&store);
        assert_eq!(recipe.title, "code-review");
        assert!(recipe.instructions.as_ref().unwrap().contains("sequence"));

        let subs = recipe.sub_recipes.unwrap();
        assert_eq!(subs.len(), 2);
        assert!(subs[0].sequential_when_repeated);

        // Must include summon extension
        let exts = recipe.extensions.unwrap();
        assert!(exts.iter().any(|e| e.name() == "summon"));
    }

    #[test]
    fn to_recipe_fan_out() {
        let (_tmp, store) = temp_store_with_defaults();
        let yaml = include_str!("../teams/research-panel.yaml");
        let team = TeamDefinition::from_yaml(yaml).unwrap();

        let recipe = team.to_recipe(&store);
        assert!(
            recipe
                .instructions
                .as_ref()
                .unwrap()
                .contains("simultaneously")
        );
        let subs = recipe.sub_recipes.unwrap();
        assert!(!subs[0].sequential_when_repeated);
    }

    #[test]
    fn to_recipe_router() {
        let (_tmp, store) = temp_store_with_defaults();
        let yaml = include_str!("../teams/smart-router.yaml");
        let team = TeamDefinition::from_yaml(yaml).unwrap();

        let recipe = team.to_recipe(&store);
        assert!(
            recipe
                .instructions
                .as_ref()
                .unwrap()
                .contains("most appropriate")
        );
    }

    #[test]
    fn validation_rejects_empty_title() {
        let yaml = r#"
version: "1.0.0"
title: "   "
workflow: chain
agents:
  - profile: developer
"#;
        let err = TeamDefinition::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("title is required"));
    }

    #[test]
    fn validation_rejects_empty_agent_profile() {
        let yaml = r#"
version: "1.0.0"
title: "test-team"
workflow: chain
agents:
  - profile: ""
"#;
        let err = TeamDefinition::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("profile name cannot be empty"));
    }

    #[test]
    fn validation_rejects_fan_out_without_config() {
        let yaml = r#"
version: "1.0.0"
title: "bad-fanout"
workflow: fan-out
agents:
  - profile: developer
"#;
        let err = TeamDefinition::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("fan-out workflow requires"));
    }

    #[test]
    fn test_name_returns_title() {
        let team = TeamDefinition {
            version: "1.0.0".into(),
            title: "my-team".into(),
            description: None,
            workflow: OrchestrationPattern::Chain,
            agents: vec![TeamAgent {
                profile: "dev".into(),
                role: None,
            }],
            router: None,
            fan_out: None,
            communication_mode: CommunicationMode::default(),
        };
        assert_eq!(team.name(), "my-team");
    }

    #[test]
    fn test_file_name() {
        let team = TeamDefinition {
            version: "1.0.0".into(),
            title: "My Cool Team".into(),
            description: None,
            workflow: OrchestrationPattern::Chain,
            agents: vec![TeamAgent {
                profile: "dev".into(),
                role: None,
            }],
            router: None,
            fan_out: None,
            communication_mode: CommunicationMode::default(),
        };
        assert_eq!(team.file_name(), "my-cool-team.yaml");
    }

    #[test]
    fn test_yaml_definition_trait_impl() {
        use opengoose_types::YamlDefinition;
        let yaml = include_str!("../teams/code-review.yaml");
        let team = <TeamDefinition as YamlDefinition>::from_yaml(yaml).unwrap();
        assert_eq!(team.title(), "code-review");
        let roundtripped = team.to_yaml().unwrap();
        let reparsed = <TeamDefinition as YamlDefinition>::from_yaml(&roundtripped).unwrap();
        assert_eq!(reparsed.title(), team.title());
    }

    #[test]
    fn test_orchestration_pattern_serde() {
        let yaml = r#"
version: "1.0.0"
title: "test"
workflow: fan-out
agents:
  - profile: dev
fan_out:
  merge_strategy: concatenate
"#;
        let team = TeamDefinition::from_yaml(yaml).unwrap();
        assert_eq!(team.workflow, OrchestrationPattern::FanOut);
    }

    #[test]
    fn test_team_with_description() {
        let yaml = r#"
version: "1.0.0"
title: "described-team"
description: "A team for testing"
workflow: chain
agents:
  - profile: dev
    role: "develop features"
"#;
        let team = TeamDefinition::from_yaml(yaml).unwrap();
        assert_eq!(team.description, Some("A team for testing".into()));
        assert_eq!(team.agents[0].role, Some("develop features".into()));
    }

    fn temp_store_with_defaults() -> (tempfile::TempDir, ProfileStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProfileStore::with_dir(tmp.path().to_path_buf());
        store.install_defaults(false).unwrap();
        (tmp, store)
    }
}
