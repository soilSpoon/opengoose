//! Tests for conversion helper functions and edge cases.

use std::collections::HashMap;

use goose::recipe::{RecipeParameterInputType, RecipeParameterRequirement};
use opengoose_profiles::{ExtensionRef, ParameterRef, ProfileSettings};

use super::super::conversion::{profile_to_recipe, recipe_to_profile, settings_to_retry_config};
use super::{empty_profile, empty_recipe};

// ── settings_to_retry_config ──────────────────────────────────────────

#[test]
fn retry_config_none_when_max_retries_unset() {
    let settings = ProfileSettings {
        goose_provider: None,
        goose_model: None,
        provider_fallbacks: vec![],
        temperature: None,
        max_turns: None,
        message_retention_days: None,
        event_retention_days: None,
        max_retries: None,
        retry_checks: vec!["cargo test".into()],
        on_failure: Some("cargo clean".into()),
    };
    assert!(settings_to_retry_config(&settings).is_none());
}

#[test]
fn retry_config_with_empty_checks() {
    let settings = ProfileSettings {
        goose_provider: None,
        goose_model: None,
        provider_fallbacks: vec![],
        temperature: None,
        max_turns: None,
        message_retention_days: None,
        event_retention_days: None,
        max_retries: Some(5),
        retry_checks: vec![],
        on_failure: None,
    };
    let config = settings_to_retry_config(&settings).unwrap();
    assert_eq!(config.max_retries, 5);
    assert!(config.checks.is_empty());
    assert!(config.on_failure.is_none());
    assert!(config.timeout_seconds.is_none());
}

// ── parse_input_type / format_input_type round-trips ──────────────────

#[test]
fn input_type_round_trips() {
    let types = ["string", "number", "boolean", "date", "file", "select"];
    for type_str in types {
        let mut profile = empty_profile("param-test");
        profile.parameters = Some(vec![ParameterRef {
            key: "k".into(),
            input_type: type_str.into(),
            requirement: "optional".into(),
            description: "test".into(),
            default: None,
        }]);
        let recipe = profile_to_recipe(&profile);
        let back = recipe_to_profile(&recipe);
        assert_eq!(
            back.parameters.as_ref().unwrap()[0].input_type,
            type_str,
            "round-trip failed for input_type={type_str}"
        );
    }
}

#[test]
fn unknown_input_type_defaults_to_string() {
    let mut profile = empty_profile("unknown-type");
    profile.parameters = Some(vec![ParameterRef {
        key: "k".into(),
        input_type: "unknown_custom_type".into(),
        requirement: "optional".into(),
        description: "test".into(),
        default: None,
    }]);
    let recipe = profile_to_recipe(&profile);
    let param = &recipe.parameters.as_ref().unwrap()[0];
    assert!(matches!(param.input_type, RecipeParameterInputType::String));
}

// ── parse_requirement / format_requirement round-trips ────────────────

#[test]
fn requirement_round_trips() {
    let reqs = ["required", "optional", "user_prompt"];
    for req_str in reqs {
        let mut profile = empty_profile("req-test");
        profile.parameters = Some(vec![ParameterRef {
            key: "k".into(),
            input_type: "string".into(),
            requirement: req_str.into(),
            description: "test".into(),
            default: None,
        }]);
        let recipe = profile_to_recipe(&profile);
        let back = recipe_to_profile(&recipe);
        assert_eq!(
            back.parameters.as_ref().unwrap()[0].requirement,
            req_str,
            "round-trip failed for requirement={req_str}"
        );
    }
}

#[test]
fn unknown_requirement_defaults_to_optional() {
    let mut profile = empty_profile("unknown-req");
    profile.parameters = Some(vec![ParameterRef {
        key: "k".into(),
        input_type: "string".into(),
        requirement: "something_else".into(),
        description: "test".into(),
        default: None,
    }]);
    let recipe = profile_to_recipe(&profile);
    let param = &recipe.parameters.as_ref().unwrap()[0];
    assert!(matches!(
        param.requirement,
        RecipeParameterRequirement::Optional
    ));
}

// ── profile_to_recipe edge cases ──────────────────────────────────────

#[test]
fn profile_with_no_description_becomes_empty_string() {
    let profile = empty_profile("no-desc");
    let recipe = profile_to_recipe(&profile);
    assert_eq!(recipe.description, "");
}

#[test]
fn profile_with_empty_extensions_produces_none() {
    let mut profile = empty_profile("no-ext");
    profile.extensions = vec![];
    let recipe = profile_to_recipe(&profile);
    assert!(recipe.extensions.is_none());
}

#[test]
fn profile_with_extensions_produces_some() {
    let mut profile = empty_profile("with-ext");
    profile.extensions = vec![ExtensionRef {
        name: "dev".into(),
        ext_type: "builtin".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    }];
    let recipe = profile_to_recipe(&profile);
    assert_eq!(recipe.extensions.as_ref().unwrap().len(), 1);
}

#[test]
fn profile_with_no_settings_produces_no_retry() {
    let profile = empty_profile("no-settings");
    let recipe = profile_to_recipe(&profile);
    assert!(recipe.settings.is_none());
    assert!(recipe.retry.is_none());
}

#[test]
fn temperature_cast_f64_to_f32() {
    let mut profile = empty_profile("temp-cast");
    profile.settings = Some(ProfileSettings {
        goose_provider: None,
        goose_model: None,
        provider_fallbacks: vec![],
        temperature: Some(0.7),
        max_turns: Some(20),
        message_retention_days: None,
        event_retention_days: None,
        max_retries: None,
        retry_checks: vec![],
        on_failure: None,
    });
    let recipe = profile_to_recipe(&profile);
    let settings = recipe.settings.unwrap();
    let temp = settings.temperature.unwrap();
    assert!((temp - 0.7f32).abs() < f32::EPSILON);
    assert_eq!(settings.max_turns, Some(20));
}

// ── recipe_to_profile edge cases ──────────────────────────────────────

#[test]
fn recipe_empty_description_becomes_none_in_profile() {
    let recipe = empty_recipe("empty-desc");
    let profile = recipe_to_profile(&recipe);
    assert!(profile.description.is_none());
}

#[test]
fn recipe_nonempty_description_becomes_some() {
    let mut recipe = empty_recipe("has-desc");
    recipe.description = "A useful agent".into();
    let profile = recipe_to_profile(&recipe);
    assert_eq!(profile.description.as_deref(), Some("A useful agent"));
}

#[test]
fn recipe_with_no_extensions_produces_empty_vec() {
    let recipe = empty_recipe("no-ext");
    let profile = recipe_to_profile(&recipe);
    assert!(profile.extensions.is_empty());
}

#[test]
fn recipe_response_with_json_schema_preserved() {
    let mut recipe = empty_recipe("response");
    recipe.response = Some(goose::recipe::Response {
        json_schema: Some(serde_json::json!({"type": "object", "properties": {}})),
    });
    let profile = recipe_to_profile(&recipe);
    assert!(profile.response.is_some());
    assert_eq!(
        profile.response.unwrap()["type"],
        serde_json::json!("object")
    );
}

#[test]
fn recipe_response_with_no_schema_produces_none() {
    let mut recipe = empty_recipe("no-schema");
    recipe.response = Some(goose::recipe::Response { json_schema: None });
    let profile = recipe_to_profile(&recipe);
    assert!(profile.response.is_none());
}

#[test]
fn parameter_default_value_preserved() {
    let mut profile = empty_profile("param-default");
    profile.parameters = Some(vec![ParameterRef {
        key: "timeout".into(),
        input_type: "number".into(),
        requirement: "optional".into(),
        description: "Timeout in seconds".into(),
        default: Some("30".into()),
    }]);
    let recipe = profile_to_recipe(&profile);
    let back = recipe_to_profile(&recipe);
    let param = &back.parameters.unwrap()[0];
    assert_eq!(param.default, Some("30".into()));
}

#[test]
fn multiple_parameters_preserved_in_order() {
    let mut profile = empty_profile("multi-params");
    profile.parameters = Some(vec![
        ParameterRef {
            key: "alpha".into(),
            input_type: "string".into(),
            requirement: "required".into(),
            description: "first".into(),
            default: None,
        },
        ParameterRef {
            key: "beta".into(),
            input_type: "number".into(),
            requirement: "optional".into(),
            description: "second".into(),
            default: Some("42".into()),
        },
        ParameterRef {
            key: "gamma".into(),
            input_type: "boolean".into(),
            requirement: "user_prompt".into(),
            description: "third".into(),
            default: None,
        },
    ]);
    let recipe = profile_to_recipe(&profile);
    let back = recipe_to_profile(&recipe);
    let params = back.parameters.unwrap();
    let keys: Vec<_> = params.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, vec!["alpha", "beta", "gamma"]);
}
