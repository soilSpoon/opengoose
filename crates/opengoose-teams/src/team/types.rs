use serde::{Deserialize, Serialize};

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

/// How agents within a team communicate with each other.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CommunicationMode {
    /// Agents communicate via the orchestrator (default).
    Orchestrated,
    /// Agents share a message bus for direct peer-to-peer messaging.
    MessageBus,
}

impl Default for CommunicationMode {
    fn default() -> Self {
        Self::Orchestrated
    }
}

/// A team definition — a YAML-serializable struct that composes agent profiles into a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamDefinition {
    pub version: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional team-level goal.
    ///
    /// Injected into agent system prompts when no `ProjectContext` is present,
    /// so the team can operate with a shared goal even without a full project.
    /// When a project is set on the `OrchestrationContext`, the project goal
    /// takes precedence over this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    pub workflow: OrchestrationPattern,
    pub agents: Vec<TeamAgent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router: Option<RouterConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_out: Option<FanOutConfig>,
    /// How agents communicate within this team (default: orchestrated).
    #[serde(default, skip_serializing_if = "is_default_communication_mode")]
    pub communication_mode: CommunicationMode,
}

fn is_default_communication_mode(mode: &CommunicationMode) -> bool {
    *mode == CommunicationMode::Orchestrated
}
