use std::collections::HashMap;

use goose::agents::RetryConfig;
use goose::agents::types::SuccessCheck;

use opengoose_profiles::{ExtensionRef, ParameterRef, ProfileSettings, SubRecipeRef};

use super::super::{profile_to_recipe, recipe_to_profile};
use super::{empty_profile, empty_recipe};

#[test]
fn profile_to_recipe_round_trip() {
    let profile = opengoose_profiles::AgentProfile {
        version: "1.0.0".into(),
        title: "test-agent".into(),
        description: Some("A test agent".into()),
        instructions: Some("You are a test agent.".into()),
        prompt: None,
        extensions: vec![
            ExtensionRef {
                name: "developer".into(),
                ext_type: "builtin".into(),
                cmd: None,
                args: vec![],
                uri: None,
                timeout: Some(300),
                envs: HashMap::new(),
                env_keys: vec![],
                code: None,
                dependencies: None,
            },
            ExtensionRef {
                name: "my-tool".into(),
                ext_type: "stdio".into(),
                cmd: Some("my-tool-bin".into()),
                args: vec!["--verbose".into()],
                uri: None,
                timeout: None,
                envs: HashMap::new(),
                env_keys: vec![],
                code: None,
                dependencies: None,
            },
        ],
        skills: vec![],
        settings: Some(ProfileSettings {
            goose_provider: Some("anthropic".into()),
            goose_model: Some("claude-sonnet-4-20250514".into()),
            provider_fallbacks: vec![],
            temperature: Some(0.7),
            max_turns: Some(10),
            message_retention_days: None,
            event_retention_days: None,
            max_retries: Some(3),
            retry_checks: vec!["cargo test".into()],
            on_failure: Some("cargo clean".into()),
        }),
        activities: Some(vec!["Review code".into(), "Fix bugs".into()]),
        response: Some(serde_json::json!({"type": "object"})),
        sub_recipes: Some(vec![SubRecipeRef {
            name: "helper".into(),
            path: "/path/to/helper.yaml".into(),
            description: Some("A helper".into()),
        }]),
        parameters: Some(vec![ParameterRef {
            key: "target".into(),
            input_type: "string".into(),
            requirement: "required".into(),
            description: "Target to analyze".into(),
            default: None,
        }]),
    };

    let recipe = profile_to_recipe(&profile);
    assert_eq!(recipe.title, "test-agent");
    assert_eq!(recipe.description, "A test agent");
    assert_eq!(
        recipe.instructions.as_deref(),
        Some("You are a test agent.")
    );
    assert_eq!(recipe.extensions.as_ref().unwrap().len(), 2);
    assert_eq!(
        recipe.settings.as_ref().unwrap().goose_provider.as_deref(),
        Some("anthropic")
    );
    assert_eq!(recipe.retry.as_ref().unwrap().max_retries, 3);
    assert_eq!(recipe.retry.as_ref().unwrap().checks.len(), 1);
    assert_eq!(
        recipe.activities.as_ref().unwrap(),
        &["Review code", "Fix bugs"]
    );
    assert!(recipe.response.is_some());
    assert_eq!(recipe.sub_recipes.as_ref().unwrap().len(), 1);
    assert_eq!(recipe.parameters.as_ref().unwrap().len(), 1);
    assert_eq!(recipe.parameters.as_ref().unwrap()[0].key, "target");

    let back = recipe_to_profile(&recipe);
    assert_eq!(back.title, profile.title);
    assert_eq!(back.instructions, profile.instructions);
    assert_eq!(back.extensions.len(), 2);
    assert_eq!(back.settings.as_ref().unwrap().max_retries, Some(3));
    assert_eq!(back.settings.as_ref().unwrap().retry_checks.len(), 1);
    assert_eq!(back.activities, profile.activities);
    assert!(back.response.is_some());
    assert_eq!(back.sub_recipes.unwrap().len(), 1);
    assert_eq!(back.parameters.unwrap()[0].key, "target");
}

#[test]
fn recipe_to_profile_minimal() {
    let recipe = goose::recipe::Recipe {
        version: "1.0.0".into(),
        title: "minimal".into(),
        description: String::new(),
        instructions: Some("Do stuff".into()),
        prompt: None,
        extensions: None,
        settings: None,
        activities: None,
        author: None,
        parameters: None,
        response: None,
        sub_recipes: None,
        retry: None,
    };

    let profile = recipe_to_profile(&recipe);
    assert_eq!(profile.title, "minimal");
    assert_eq!(profile.description, None);
    assert!(profile.extensions.is_empty());
    assert!(profile.settings.is_none());
    assert!(profile.activities.is_none());
    assert!(profile.response.is_none());
    assert!(profile.sub_recipes.is_none());
    assert!(profile.parameters.is_none());
}

#[test]
fn profile_to_recipe_preserves_sub_recipe_order_and_defaults_sequential_flag() {
    let mut profile = empty_profile("sub-recipes");
    profile.activities = Some(vec!["Prepare".into(), "Execute".into()]);
    profile.sub_recipes = Some(vec![
        SubRecipeRef {
            name: "alpha".into(),
            path: "recipes/alpha.yaml".into(),
            description: Some("Alpha".into()),
        },
        SubRecipeRef {
            name: "beta".into(),
            path: "recipes/beta.yaml".into(),
            description: None,
        },
    ]);

    let recipe = profile_to_recipe(&profile);
    let sub_recipes = recipe.sub_recipes.expect("sub-recipes should be mapped");
    let names: Vec<_> = sub_recipes.iter().map(|sub| sub.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
    assert!(sub_recipes.iter().all(|sub| !sub.sequential_when_repeated));
    assert!(sub_recipes.iter().all(|sub| sub.values.is_none()));
    assert_eq!(
        recipe.activities.as_deref(),
        Some(&["Prepare".to_string(), "Execute".to_string()][..])
    );
}

#[test]
fn recipe_to_profile_uses_retry_config_even_without_settings() {
    let mut recipe = empty_recipe("retry-only");
    recipe.retry = Some(RetryConfig {
        max_retries: 2,
        checks: vec![
            SuccessCheck::Shell {
                command: "cargo fmt --check".into(),
            },
            SuccessCheck::Shell {
                command: "cargo test".into(),
            },
        ],
        on_failure: Some("cargo clean".into()),
        timeout_seconds: Some(120),
        on_failure_timeout_seconds: Some(240),
    });

    let profile = recipe_to_profile(&recipe);
    let settings = profile
        .settings
        .expect("retry should create profile settings");
    assert_eq!(settings.max_retries, Some(2));
    assert_eq!(
        settings.retry_checks,
        vec!["cargo fmt --check".to_string(), "cargo test".to_string()]
    );
    assert_eq!(settings.on_failure.as_deref(), Some("cargo clean"));
    assert!(settings.goose_provider.is_none());
    assert!(settings.goose_model.is_none());
}
