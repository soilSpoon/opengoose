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
        format!("{}.yaml", self.title.to_lowercase().replace(' ', "-"))
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
            return Err(ProfileError::ValidationFailed("title is required".into()));
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

    #[test]
    fn test_name_returns_title() {
        let profile = AgentProfile {
            version: "1.0.0".into(),
            title: "My Agent".into(),
            description: None,
            instructions: Some("do stuff".into()),
            prompt: None,
            extensions: vec![],
            settings: None,
        };
        assert_eq!(profile.name(), "My Agent");
    }

    #[test]
    fn test_file_name_format() {
        let profile = AgentProfile {
            version: "1.0.0".into(),
            title: "My Cool Agent".into(),
            description: None,
            instructions: Some("do stuff".into()),
            prompt: None,
            extensions: vec![],
            settings: None,
        };
        assert_eq!(profile.file_name(), "my-cool-agent.yaml");
    }

    #[test]
    fn test_validation_accepts_prompt_only() {
        let yaml = r#"
version: "1.0.0"
title: "test"
prompt: "Hello, I am a bot."
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert!(profile.instructions.is_none());
        assert_eq!(profile.prompt.as_deref(), Some("Hello, I am a bot."));
    }

    #[test]
    fn test_profile_with_settings() {
        let yaml = r#"
version: "1.0.0"
title: "custom-agent"
instructions: "Do things"
settings:
  goose_provider: anthropic
  goose_model: claude-sonnet-4-20250514
  temperature: 0.5
  max_turns: 5
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        let settings = profile.settings.unwrap();
        assert_eq!(settings.goose_provider.as_deref(), Some("anthropic"));
        assert_eq!(settings.goose_model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(settings.temperature, Some(0.5));
        assert_eq!(settings.max_turns, Some(5));
    }

    #[test]
    fn test_profile_with_extensions() {
        let yaml = r#"
version: "1.0.0"
title: "ext-agent"
instructions: "Use tools"
extensions:
  - name: developer
    type: builtin
    timeout: 300
  - name: my-tool
    type: stdio
    cmd: my-binary
    args:
      - --verbose
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(profile.extensions.len(), 2);
        assert_eq!(profile.extensions[0].name, "developer");
        assert_eq!(profile.extensions[0].ext_type, "builtin");
        assert_eq!(profile.extensions[0].timeout, Some(300));
        assert_eq!(profile.extensions[1].name, "my-tool");
        assert_eq!(profile.extensions[1].ext_type, "stdio");
        assert_eq!(profile.extensions[1].cmd.as_deref(), Some("my-binary"));
        assert_eq!(profile.extensions[1].args, vec!["--verbose"]);
    }

    #[test]
    fn test_profile_with_description() {
        let yaml = r#"
version: "1.0.0"
title: "desc-agent"
description: "An agent with a description"
instructions: "Do stuff"
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(
            profile.description.as_deref(),
            Some("An agent with a description")
        );
    }

    #[test]
    fn test_yaml_definition_trait() {
        use opengoose_types::YamlDefinition;
        let yaml = include_str!("../profiles/researcher.yaml");
        let profile = <AgentProfile as YamlDefinition>::from_yaml(yaml).unwrap();
        assert_eq!(profile.title(), "researcher");
        let file_name = profile.file_name();
        assert_eq!(file_name, "researcher.yaml");
    }

    #[test]
    fn test_invalid_yaml_returns_error() {
        let yaml = "not: valid: yaml: [[[";
        let result = AgentProfile::from_yaml(yaml);
        assert!(result.is_err());
    }
}
