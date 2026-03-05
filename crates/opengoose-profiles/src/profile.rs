use serde::{Deserialize, Serialize};

use crate::error::{ProfileError, ProfileResult};

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

/// Model and provider settings.
///
/// Aligns with Goose's Recipe `settings` block so profiles can be used
/// interchangeably with Goose recipes.
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
    /// Maximum retry attempts for automated validation (maps to Goose RetryConfig).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    /// Shell commands to validate success (maps to Goose SuccessCheck::Shell).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retry_checks: Vec<String>,
    /// Shell command to run on failure for cleanup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
}

/// An agent profile — a YAML-serializable struct compatible with Goose's Recipe schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub version: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ExtensionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProfileSettings>,
}

impl AgentProfile {
    /// Profile name (the title, lowercased).
    pub fn name(&self) -> &str {
        &self.title
    }

    /// File-safe name: lowercase, spaces replaced with hyphens.
    pub fn file_name(&self) -> String {
        format!(
            "{}.yaml",
            self.title.to_lowercase().replace(' ', "-")
        )
    }

    /// Parse from YAML string.
    pub fn from_yaml(yaml: &str) -> ProfileResult<Self> {
        let profile: Self = serde_yaml::from_str(yaml)?;
        profile.validate()?;
        Ok(profile)
    }

    /// Serialize to YAML string.
    pub fn to_yaml(&self) -> ProfileResult<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// Validate required fields.
    pub fn validate(&self) -> ProfileResult<()> {
        if self.title.trim().is_empty() {
            return Err(ProfileError::ValidationFailed(
                "title is required".into(),
            ));
        }
        if self.instructions.is_none() && self.prompt.is_none() {
            return Err(ProfileError::ValidationFailed(
                "either `instructions` or `prompt` is required".into(),
            ));
        }
        Ok(())
    }
}

impl opengoose_types::YamlDefinition for AgentProfile {
    type Error = ProfileError;

    fn title(&self) -> &str {
        &self.title
    }

    fn from_yaml(yaml: &str) -> ProfileResult<Self> {
        AgentProfile::from_yaml(yaml)
    }

    fn to_yaml(&self) -> ProfileResult<String> {
        AgentProfile::to_yaml(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_yaml() {
        let yaml = include_str!("../profiles/researcher.yaml");
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(profile.name(), "researcher");
        assert!(profile.instructions.is_some());

        let serialized = profile.to_yaml().unwrap();
        let reparsed = AgentProfile::from_yaml(&serialized).unwrap();
        assert_eq!(reparsed.title, profile.title);
    }

    #[test]
    fn validation_rejects_empty_title() {
        let yaml = r#"
version: "1.0.0"
title: ""
instructions: "hello"
"#;
        let err = AgentProfile::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("title is required"));
    }

    #[test]
    fn validation_rejects_no_instructions_or_prompt() {
        let yaml = r#"
version: "1.0.0"
title: "test"
"#;
        let err = AgentProfile::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("instructions"));
    }
}
