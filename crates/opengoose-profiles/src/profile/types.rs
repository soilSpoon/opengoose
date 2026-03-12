use serde::{Deserialize, Serialize};

/// Extension reference within a profile (matches Goose Recipe extension format).
///
/// Supports the same extension types as Goose recipes: `builtin`, `stdio`,
/// `streamable_http`, `platform`, and `inline_python`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionRef {
    pub name: String,
    #[serde(rename = "type")]
    pub ext_type: String,
    /// Command to run (required for `stdio` type).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,
    /// Arguments for the command (`stdio` type).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// URI endpoint (required for `streamable_http` type).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Timeout in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Environment variables for the extension process.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub envs: std::collections::HashMap<String, String>,
    /// Secret keys to resolve from the environment (`stdio` / `streamable_http`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_keys: Vec<String>,
    /// Python code to execute (required for `inline_python` type).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Python package dependencies (for `inline_python` type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
}

/// Sub-recipe reference (simplified version of Goose's SubRecipe).
///
/// Allows a profile to declare sub-recipes that can be executed via
/// Goose's Summon extension when the profile is used as a Recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubRecipeRef {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_input_type() -> String {
    "string".to_string()
}

fn default_requirement() -> String {
    "optional".to_string()
}

/// Parameter definition (simplified version of Goose's RecipeParameter).
///
/// Allows a profile to declare input parameters that can be filled at
/// runtime, enabling reusable, parameterized agent configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterRef {
    pub key: String,
    #[serde(default = "default_input_type")]
    pub input_type: String,
    #[serde(default = "default_requirement")]
    pub requirement: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Model and provider settings.
///
/// Aligns with Goose's Recipe `settings` block so profiles can be used
/// interchangeably with Goose recipes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderFallback {
    pub goose_provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goose_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goose_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goose_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Retain persisted session messages for at most this many days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_retention_days: Option<u32>,
    /// Retain persisted event history for at most this many days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_retention_days: Option<u32>,
    /// Maximum retry attempts for automated validation (maps to Goose RetryConfig).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    /// Shell commands to validate success (maps to Goose SuccessCheck::Shell).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retry_checks: Vec<String>,
    /// Shell command to run on failure for cleanup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    /// Ordered fallback providers/models to try when the primary fails.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_fallbacks: Vec<ProviderFallback>,
}

impl ProfileSettings {
    pub fn is_empty(&self) -> bool {
        self.goose_provider.is_none()
            && self.goose_model.is_none()
            && self.temperature.is_none()
            && self.max_turns.is_none()
            && self.message_retention_days.is_none()
            && self.event_retention_days.is_none()
            && self.max_retries.is_none()
            && self.retry_checks.is_empty()
            && self.on_failure.is_none()
            && self.provider_fallbacks.is_empty()
    }
}
