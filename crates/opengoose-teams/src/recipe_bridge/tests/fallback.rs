use goose::agents::extension::ExtensionConfig;
use goose::recipe::{RecipeParameterInputType, RecipeParameterRequirement, Response};

use opengoose_profiles::{ParameterRef, ProfileSettings};

use super::super::{
    config_to_ext_ref, profile_to_recipe, recipe_to_profile, settings_to_retry_config,
};
use super::{empty_profile, empty_recipe};

#[test]
fn settings_to_retry_config_requires_max_retries() {
    let settings = ProfileSettings {
        goose_provider: Some("anthropic".into()),
        goose_model: Some("claude-sonnet".into()),
        temperature: Some(0.4),
        max_turns: Some(8),
        message_retention_days: Some(7),
        event_retention_days: Some(14),
        max_retries: None,
        retry_checks: vec!["cargo test".into()],
        on_failure: Some("cargo clean".into()),
        provider_fallbacks: vec![],
    };

    assert!(settings_to_retry_config(&settings).is_none());
}

#[test]
fn profile_to_recipe_defaults_unknown_parameter_kinds_to_string_optional() {
    let mut profile = empty_profile("parameter-fallbacks");
    profile.parameters = Some(vec![ParameterRef {
        key: "target".into(),
        input_type: "mystery".into(),
        requirement: "surprise".into(),
        description: "Target input".into(),
        default: Some("src".into()),
    }]);

    let recipe = profile_to_recipe(&profile);
    let parameter = &recipe.parameters.as_ref().unwrap()[0];
    assert!(matches!(
        parameter.input_type,
        RecipeParameterInputType::String
    ));
    assert!(matches!(
        parameter.requirement,
        RecipeParameterRequirement::Optional
    ));
    assert_eq!(parameter.default.as_deref(), Some("src"));
}

#[test]
fn recipe_to_profile_drops_unmapped_extensions_and_missing_json_schema() {
    let mut recipe = empty_recipe("recipe-ext-filter");
    recipe.extensions = Some(vec![
        ExtensionConfig::Sse {
            name: "legacy-sse".into(),
            description: String::new(),
            uri: Some("https://example.invalid/sse".into()),
        },
        ExtensionConfig::Frontend {
            name: "frontend".into(),
            description: String::new(),
            tools: vec![],
            instructions: Some("use the browser".into()),
            bundled: None,
            available_tools: vec![],
        },
        ExtensionConfig::Builtin {
            name: "developer".into(),
            description: String::new(),
            display_name: None,
            timeout: Some(60),
            bundled: Some(true),
            available_tools: vec![],
        },
    ]);
    recipe.response = Some(Response { json_schema: None });

    let profile = recipe_to_profile(&recipe);
    assert_eq!(profile.extensions.len(), 1);
    assert_eq!(profile.extensions[0].name, "developer");
    assert_eq!(profile.extensions[0].ext_type, "builtin");
    assert!(profile.response.is_none());
}

#[test]
fn config_to_ext_ref_skips_frontend_extensions() {
    let frontend = ExtensionConfig::Frontend {
        name: "frontend".into(),
        description: String::new(),
        tools: vec![],
        instructions: None,
        bundled: None,
        available_tools: vec![],
    };

    assert!(config_to_ext_ref(&frontend).is_none());
}
