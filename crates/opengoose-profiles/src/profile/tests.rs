use super::*;

#[test]
fn round_trip_yaml() {
    let yaml = include_str!("../../profiles/researcher.yaml");
    let profile = AgentProfile::from_yaml(yaml).unwrap();
    assert_eq!(profile.name(), "researcher");

    let serialized = profile.to_yaml().unwrap();
    let reparsed = AgentProfile::from_yaml(&serialized).unwrap();
    assert_eq!(reparsed.title, profile.title);
}

#[test]
fn validation_rejects_empty_title() {
    let yaml = r#"
version: "1.0.0"
title: ""
"#;
    let err = AgentProfile::from_yaml(yaml).unwrap_err();
    assert!(err.to_string().contains("title is required"));
}

#[test]
fn validation_accepts_profile_without_instructions() {
    let yaml = r#"
version: "1.0.0"
title: "test"
"#;
    let profile = AgentProfile::from_yaml(yaml).unwrap();
    assert!(profile.instructions.is_none());
    assert!(profile.prompt.is_none());
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
    let yaml = include_str!("../../profiles/developer.yaml");
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
        instructions: None,
        prompt: None,
        extensions: vec![],
        skills: vec![],
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
        instructions: None,
        prompt: None,
        extensions: vec![],
        skills: vec![],
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
  message_retention_days: 30
  event_retention_days: 14
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
    assert_eq!(settings.message_retention_days, Some(30));
    assert_eq!(settings.event_retention_days, Some(14));
}

#[test]
fn test_profile_with_provider_fallbacks() {
    let yaml = r#"
version: "1.0.0"
title: "fallback-agent"
instructions: "Fail over if needed"
settings:
  goose_provider: anthropic
  goose_model: claude-sonnet-4-20250514
  provider_fallbacks:
    - goose_provider: openai
      goose_model: gpt-4.1
    - goose_provider: xai
"#;
    let profile = AgentProfile::from_yaml(yaml).unwrap();
    let settings = profile.settings.unwrap();
    assert_eq!(settings.provider_fallbacks.len(), 2);
    assert_eq!(settings.provider_fallbacks[0].goose_provider, "openai");
    assert_eq!(
        settings.provider_fallbacks[0].goose_model.as_deref(),
        Some("gpt-4.1")
    );
    assert_eq!(settings.provider_fallbacks[1].goose_provider, "xai");
    assert!(settings.provider_fallbacks[1].goose_model.is_none());
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
    let yaml = include_str!("../../profiles/researcher.yaml");
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
fn test_profile_settings_is_empty() {
    assert!(ProfileSettings::default().is_empty());

    let settings = ProfileSettings {
        message_retention_days: Some(14),
        event_retention_days: Some(30),
        ..ProfileSettings::default()
    };
    assert!(!settings.is_empty());
}

#[test]
fn with_model_override_sets_goose_model() {
    let profile = AgentProfile::from_yaml(
        r#"
version: "1.0.0"
title: "main"
"#,
    )
    .unwrap();

    let overridden = profile.with_model_override(Some("gpt-5-mini"));
    assert_eq!(
        overridden
            .settings
            .as_ref()
            .and_then(|settings| settings.goose_model.as_deref()),
        Some("gpt-5-mini")
    );
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
"#;
    let err = AgentProfile::from_yaml(yaml).unwrap_err();
    assert!(err.to_string().contains("title is required"));
}

#[test]
fn skills_field_round_trips() {
    let yaml = r#"
version: "1.0.0"
title: "skilled-agent"
instructions: "Uses skills"
skills:
  - git-tools
  - web-search
"#;
    let profile = AgentProfile::from_yaml(yaml).unwrap();
    assert_eq!(profile.skills, vec!["git-tools", "web-search"]);

    let serialized = profile.to_yaml().unwrap();
    let reparsed = AgentProfile::from_yaml(&serialized).unwrap();
    assert_eq!(reparsed.skills, vec!["git-tools", "web-search"]);
}

#[test]
fn profile_without_skills_has_empty_skills_vec() {
    let yaml = r#"
version: "1.0.0"
title: "plain-agent"
"#;
    let profile = AgentProfile::from_yaml(yaml).unwrap();
    assert!(profile.skills.is_empty());
}

#[test]
fn resolve_extensions_no_skills() {
    let yaml = r#"
version: "1.0.0"
title: "no-skill-agent"
extensions:
  - name: developer
    type: builtin
"#;
    let profile = AgentProfile::from_yaml(yaml).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let store = crate::SkillStore::with_dir(tmp.path().to_path_buf());
    let exts = profile.resolve_extensions(&store).unwrap();
    assert_eq!(exts.len(), 1);
    assert_eq!(exts[0].name, "developer");
}

#[test]
fn resolve_extensions_merges_and_deduplicates() {
    use crate::SkillStore;
    let tmp = tempfile::tempdir().unwrap();
    let store = SkillStore::with_dir(tmp.path().to_path_buf());

    let skill_yaml = r#"
name: my-skill
version: "1.0.0"
extensions:
  - name: shared-tool
    type: builtin
  - name: extra-tool
    type: builtin
"#;
    let skill = crate::Skill::from_yaml(skill_yaml).unwrap();
    store.save(&skill, false).unwrap();

    // Profile has its own `shared-tool` which should win.
    let yaml = r#"
version: "1.0.0"
title: "merged-agent"
extensions:
  - name: shared-tool
    type: stdio
    cmd: my-binary
skills:
  - my-skill
"#;
    let profile = AgentProfile::from_yaml(yaml).unwrap();
    let exts = profile.resolve_extensions(&store).unwrap();

    assert_eq!(exts.len(), 2);
    // profile's own `shared-tool` (stdio) comes first and wins.
    assert_eq!(exts[0].name, "shared-tool");
    assert_eq!(exts[0].ext_type, "stdio");
    // skill's `extra-tool` is appended.
    assert_eq!(exts[1].name, "extra-tool");
}
