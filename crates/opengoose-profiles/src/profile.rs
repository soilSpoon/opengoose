use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

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
    /// Activity pills displayed when loading the profile (maps to Recipe `activities`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activities: Option<Vec<String>>,
    /// JSON schema for structured output (maps to Recipe `response.json_schema`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<JsonValue>,
    /// Sub-recipes this profile can delegate to via Goose's Summon extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_recipes: Option<Vec<SubRecipeRef>>,
    /// Input parameters for parameterized execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ParameterRef>>,
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
    fn new_recipe_fields_round_trip() {
        let yaml = r#"
version: "1.0.0"
title: "advanced-agent"
instructions: "Do things"
activities:
  - "Analyze code"
  - "Write tests"
response:
  type: object
  properties:
    result:
      type: string
sub_recipes:
  - name: helper
    path: /path/to/helper.yaml
    description: "A helper agent"
parameters:
  - key: project_name
    input_type: string
    requirement: required
    description: "Name of the project"
    default: my-project
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(
            profile.activities.as_ref().unwrap(),
            &["Analyze code", "Write tests"]
        );
        assert!(profile.response.is_some());
        let subs = profile.sub_recipes.as_ref().unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].name, "helper");
        let params = profile.parameters.as_ref().unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].key, "project_name");
        assert_eq!(params[0].requirement, "required");
        assert_eq!(params[0].default.as_deref(), Some("my-project"));

        // Round-trip
        let serialized = profile.to_yaml().unwrap();
        let reparsed = AgentProfile::from_yaml(&serialized).unwrap();
        assert_eq!(reparsed.activities, profile.activities);
        assert_eq!(reparsed.sub_recipes.unwrap().len(), 1);
        assert_eq!(reparsed.parameters.unwrap()[0].key, "project_name");
    }

    #[test]
    fn existing_profiles_unaffected_by_new_fields() {
        let yaml = include_str!("../profiles/developer.yaml");
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert!(profile.activities.is_none());
        assert!(profile.response.is_none());
        assert!(profile.sub_recipes.is_none());
        assert!(profile.parameters.is_none());
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
            activities: None,
            response: None,
            sub_recipes: None,
            parameters: None,
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
            activities: None,
            response: None,
            sub_recipes: None,
            parameters: None,
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
        assert_eq!(
            settings.goose_model.as_deref(),
            Some("claude-sonnet-4-20250514")
        );
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

    #[test]
    fn test_profile_with_both_instructions_and_prompt() {
        // Having both `instructions` and `prompt` is valid.
        let yaml = r#"
version: "1.0.0"
title: "dual-agent"
instructions: "System instructions here"
prompt: "Initial prompt here"
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(
            profile.instructions.as_deref(),
            Some("System instructions here")
        );
        assert_eq!(profile.prompt.as_deref(), Some("Initial prompt here"));
    }

    #[test]
    fn test_profile_settings_retry_config() {
        // Retry-related fields (max_retries, retry_checks, on_failure) should round-trip.
        let yaml = r#"
version: "1.0.0"
title: "retry-agent"
instructions: "Do things with retries"
settings:
  max_retries: 5
  retry_checks:
    - "cargo test"
    - "cargo clippy"
  on_failure: "cargo clean"
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        let settings = profile.settings.as_ref().unwrap();
        assert_eq!(settings.max_retries, Some(5));
        assert_eq!(settings.retry_checks, vec!["cargo test", "cargo clippy"]);
        assert_eq!(settings.on_failure.as_deref(), Some("cargo clean"));

        // Round-trip
        let serialized = profile.to_yaml().unwrap();
        let reparsed = AgentProfile::from_yaml(&serialized).unwrap();
        let rs = reparsed.settings.unwrap();
        assert_eq!(rs.max_retries, Some(5));
        assert_eq!(rs.retry_checks.len(), 2);
        assert_eq!(rs.on_failure.as_deref(), Some("cargo clean"));
    }

    #[test]
    fn test_parameter_ref_defaults() {
        // When input_type and requirement are omitted, they should use defaults.
        let yaml = r#"
version: "1.0.0"
title: "param-agent"
instructions: "Uses parameters"
parameters:
  - key: name
    description: "User name"
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        let params = profile.parameters.unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].key, "name");
        assert_eq!(params[0].input_type, "string");
        assert_eq!(params[0].requirement, "optional");
        assert!(params[0].default.is_none());
    }

    #[test]
    fn test_extension_ref_envs_and_env_keys() {
        // Test envs map and env_keys list on an extension.
        let yaml = r#"
version: "1.0.0"
title: "env-agent"
instructions: "Use external tool"
extensions:
  - name: my-tool
    type: stdio
    cmd: my-binary
    envs:
      MY_VAR: "hello"
      OTHER_VAR: "world"
    env_keys:
      - API_KEY
      - SECRET_TOKEN
"#;
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(profile.extensions.len(), 1);
        let ext = &profile.extensions[0];
        assert_eq!(ext.envs.get("MY_VAR").unwrap(), "hello");
        assert_eq!(ext.envs.get("OTHER_VAR").unwrap(), "world");
        assert_eq!(ext.env_keys, vec!["API_KEY", "SECRET_TOKEN"]);

        // Round-trip
        let serialized = profile.to_yaml().unwrap();
        let reparsed = AgentProfile::from_yaml(&serialized).unwrap();
        let ext2 = &reparsed.extensions[0];
        assert_eq!(ext2.envs.len(), 2);
        assert_eq!(ext2.env_keys.len(), 2);
    }

    #[test]
    fn test_validation_rejects_whitespace_only_title() {
        // A title that is only whitespace should be rejected.
        let yaml = r#"
version: "1.0.0"
title: "   "
instructions: "hello"
"#;
        let err = AgentProfile::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("title is required"));
    }
}
