use serde::{Deserialize, Serialize};

/// A complete workflow definition loaded from YAML.
///
/// Mirrors antfarm's approach: a named workflow containing a sequence of
/// steps, each assigned to a specialized agent persona.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDef {
    /// Unique identifier for this workflow (e.g. "feature-dev", "bug-fix").
    pub name: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,

    /// Agent personas available to this workflow.
    pub agents: Vec<AgentDef>,

    /// Ordered list of steps to execute.
    pub steps: Vec<StepDef>,
}

/// An agent persona that can be assigned to workflow steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    /// Short identifier referenced by steps (e.g. "architect", "developer").
    pub id: String,

    /// Display name shown in UI and logs.
    pub name: String,

    /// System prompt / persona description sent to the LLM.
    pub system_prompt: String,
}

/// A single step in a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDef {
    /// Step identifier (e.g. "decompose", "implement", "review").
    pub id: String,

    /// Human-readable label.
    pub name: String,

    /// Which agent persona executes this step.
    pub agent: String,

    /// Prompt template sent to the agent. Supports `{{variable}}` placeholders
    /// that get substituted with context from previous steps.
    pub prompt: String,

    /// Acceptance criteria — the next step's agent verifies these.
    #[serde(default)]
    pub expects: Vec<String>,

    /// Maximum retry attempts before escalating.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// IDs of steps whose output is injected as context.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_max_retries() -> u32 {
    2
}
