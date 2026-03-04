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

    /// Acceptance criteria — automatically appended to the prompt so the agent
    /// knows what success looks like. Also used for verification steps.
    #[serde(default)]
    pub expects: Vec<String>,

    /// Maximum retry attempts before escalating.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// IDs of steps whose output is injected as context.
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// What to do when the step fails after exhausting retries.
    #[serde(default)]
    pub on_fail: OnFailStrategy,

    /// Optional loop configuration — makes this step iterate over items
    /// from a prior step's output (antfarm's `type: loop` pattern).
    #[serde(default, rename = "loop")]
    pub loop_config: Option<LoopConfig>,

    /// Optional per-step timeout in seconds. If the step takes longer
    /// than this, it is treated as a retry-able failure.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,

    /// Optional condition — step is only executed when this evaluates true.
    /// Supports simple equality checks against context values:
    /// e.g. `"{{verdict}} == PASS"` or `"{{status}} != skip"`.
    /// If the condition is false, the step is skipped.
    #[serde(default)]
    pub when: Option<String>,
}

/// Configuration for a loop step that iterates over items from a prior step.
///
/// Antfarm uses this for patterns like: a planner produces STORIES_JSON,
/// then a fixer step loops over each story, optionally verified after each.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    /// Context key containing a JSON array of items to iterate over.
    /// E.g. "stories_json" means look for `stories_json` in context.
    pub over: String,

    /// If true, run the `verify_step` after each iteration. If the verifier
    /// returns `STATUS: retry`, the current item is re-attempted.
    #[serde(default)]
    pub verify_each: bool,

    /// Step ID of the verification step to run after each iteration.
    /// This step is re-executed inline (not as a top-level step) using its
    /// agent and prompt. The verifier can access `{{current_item}}` and
    /// `{{iteration_output}}` placeholders.
    #[serde(default)]
    pub verify_step: Option<String>,
}

/// Failure handling strategy for a step (modeled after antfarm).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnFailStrategy {
    /// Stop the entire workflow (default).
    Abort,
    /// Skip this step and continue to the next.
    Skip,
}

impl Default for OnFailStrategy {
    fn default() -> Self {
        Self::Abort
    }
}

fn default_max_retries() -> u32 {
    2
}
