use std::collections::HashMap;
use std::sync::{Arc, Barrier};
use std::thread;

use goose::agents::RetryConfig;
use goose::agents::extension::{Envs, ExtensionConfig};
use goose::agents::types::SuccessCheck;
use goose::recipe::{Recipe, RecipeParameterInputType, RecipeParameterRequirement, Response};

use opengoose_profiles::{AgentProfile, ExtensionRef, ParameterRef, ProfileSettings, SubRecipeRef};

use super::*;

fn empty_profile(title: &str) -> AgentProfile {
    AgentProfile {
        version: "1.0.0".into(),
        title: title.into(),
        description: None,
        instructions: Some("Test instructions".into()),
        prompt: None,
        extensions: vec![],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    }
}

fn empty_recipe(title: &str) -> Recipe {
    Recipe {
        version: "1.0.0".into(),
        title: title.into(),
        description: String::new(),
        instructions: Some("Test instructions".into()),
        prompt: None,
        extensions: None,
        settings: None,
        activities: None,
        author: None,
        parameters: None,
        response: None,
        sub_recipes: None,
        retry: None,
    }
}

#[test]
fn profile_to_recipe_round_trip() {
    let profile = AgentProfile {
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

    // Round-trip back to profile
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
    let recipe = Recipe {
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
    assert_eq!(profile.description, None); // empty string → None
    assert!(profile.extensions.is_empty());
    assert!(profile.settings.is_none());
    assert!(profile.activities.is_none());
    assert!(profile.response.is_none());
    assert!(profile.sub_recipes.is_none());
    assert!(profile.parameters.is_none());
}

#[test]
fn inline_python_round_trip() {
    let profile = AgentProfile {
        version: "1.0.0".into(),
        title: "py-agent".into(),
        description: None,
        instructions: Some("Run python".into()),
        prompt: None,
        extensions: vec![ExtensionRef {
            name: "analyzer".into(),
            ext_type: "inline_python".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: Some(60),
            envs: HashMap::new(),
            env_keys: vec![],
            code: Some("print('hello')".into()),
            dependencies: Some(vec!["numpy".into()]),
        }],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    };

    let recipe = profile_to_recipe(&profile);
    let exts = recipe.extensions.as_ref().unwrap();
    assert_eq!(exts.len(), 1);
    match &exts[0] {
        ExtensionConfig::InlinePython {
            name,
            code,
            dependencies,
            ..
        } => {
            assert_eq!(name, "analyzer");
            assert_eq!(code, "print('hello')");
            assert_eq!(dependencies.as_ref().unwrap(), &vec!["numpy".to_string()]);
        }
        other => unreachable!("expected InlinePython, got {:?}", other),
    }

    let back = recipe_to_profile(&recipe);
    assert_eq!(back.extensions[0].ext_type, "inline_python");
    assert_eq!(back.extensions[0].code.as_deref(), Some("print('hello')"));
}

#[test]
fn platform_extension_round_trip() {
    let profile = AgentProfile {
        version: "1.0.0".into(),
        title: "summon-agent".into(),
        description: None,
        instructions: Some("Use summon".into()),
        prompt: None,
        extensions: vec![ExtensionRef {
            name: "summon".into(),
            ext_type: "platform".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        }],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    };

    let recipe = profile_to_recipe(&profile);
    match &recipe.extensions.as_ref().unwrap()[0] {
        ExtensionConfig::Platform { name, .. } => assert_eq!(name, "summon"),
        other => unreachable!("expected Platform, got {:?}", other),
    }

    let back = recipe_to_profile(&recipe);
    assert_eq!(back.extensions[0].ext_type, "platform");
    assert_eq!(back.extensions[0].name, "summon");
}

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
fn profile_to_recipe_skips_unsupported_or_incomplete_extensions() {
    let mut profile = empty_profile("invalid-extensions");
    profile.extensions = vec![
        ExtensionRef {
            name: "builtin".into(),
            ext_type: "builtin".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: Some(30),
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "stdio-missing-cmd".into(),
            ext_type: "stdio".into(),
            cmd: None,
            args: vec!["--flag".into()],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "http-missing-uri".into(),
            ext_type: "streamable_http".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "python-missing-code".into(),
            ext_type: "inline_python".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "unsupported".into(),
            ext_type: "frontend".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
    ];

    let recipe = profile_to_recipe(&profile);
    let extensions = recipe.extensions.expect("valid builtin should remain");
    assert_eq!(extensions.len(), 1);
    match &extensions[0] {
        ExtensionConfig::Builtin { name, timeout, .. } => {
            assert_eq!(name, "builtin");
            assert_eq!(*timeout, Some(30));
        }
        other => unreachable!("expected Builtin, got {:?}", other),
    }
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
fn ext_ref_to_config_requires_required_fields() {
    let missing_stdio_cmd = ExtensionRef {
        name: "stdio".into(),
        ext_type: "stdio".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };
    let missing_http_uri = ExtensionRef {
        name: "http".into(),
        ext_type: "streamable_http".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };
    let missing_python_code = ExtensionRef {
        name: "python".into(),
        ext_type: "inline_python".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };
    let unsupported = ExtensionRef {
        name: "unsupported".into(),
        ext_type: "frontend".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };

    assert!(ext_ref_to_config(&missing_stdio_cmd).is_none());
    assert!(ext_ref_to_config(&missing_http_uri).is_none());
    assert!(ext_ref_to_config(&missing_python_code).is_none());
    assert!(ext_ref_to_config(&unsupported).is_none());
}

#[test]
fn config_to_ext_ref_preserves_sanitized_envs_and_env_keys() {
    let config = ExtensionConfig::Stdio {
        name: "tool".into(),
        description: String::new(),
        cmd: "tool-bin".into(),
        args: vec!["--json".into()],
        envs: Envs::new(HashMap::from([
            ("PATH".to_string(), "/tmp/bin".to_string()),
            ("API_KEY".to_string(), "secret".to_string()),
        ])),
        env_keys: vec!["API_KEY".into()],
        timeout: Some(45),
        bundled: None,
        available_tools: vec![],
    };

    let ext = config_to_ext_ref(&config).expect("stdio config should map to profile");
    assert_eq!(ext.ext_type, "stdio");
    assert_eq!(ext.cmd.as_deref(), Some("tool-bin"));
    assert_eq!(ext.args, vec!["--json".to_string()]);
    assert_eq!(ext.timeout, Some(45));
    assert_eq!(ext.env_keys, vec!["API_KEY".to_string()]);
    assert_eq!(
        ext.envs,
        HashMap::from([("API_KEY".to_string(), "secret".to_string())])
    );
}

#[test]
fn config_to_ext_ref_skips_sse_and_frontend_extensions() {
    let sse = ExtensionConfig::Sse {
        name: "legacy-sse".into(),
        description: String::new(),
        uri: Some("https://example.invalid/sse".into()),
    };
    let frontend = ExtensionConfig::Frontend {
        name: "frontend".into(),
        description: String::new(),
        tools: vec![],
        instructions: None,
        bundled: None,
        available_tools: vec![],
    };

    assert!(config_to_ext_ref(&sse).is_none());
    assert!(config_to_ext_ref(&frontend).is_none());
}

#[test]
fn profile_and_recipe_round_trip_is_stable_under_concurrency() {
    let mut profile = empty_profile("concurrent");
    profile.description = Some("Concurrent profile".into());
    profile.extensions = vec![
        ExtensionRef {
            name: "developer".into(),
            ext_type: "builtin".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: Some(30),
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "runner".into(),
            ext_type: "stdio".into(),
            cmd: Some("runner".into()),
            args: vec!["--fast".into()],
            uri: None,
            timeout: Some(15),
            envs: HashMap::new(),
            env_keys: vec!["RUNNER_TOKEN".into()],
            code: None,
            dependencies: None,
        },
    ];
    profile.settings = Some(ProfileSettings {
        goose_provider: Some("anthropic".into()),
        goose_model: Some("claude-sonnet".into()),
        temperature: Some(0.2),
        max_turns: Some(6),
        message_retention_days: None,
        event_retention_days: None,
        max_retries: Some(2),
        retry_checks: vec!["cargo test".into()],
        on_failure: Some("cargo clean".into()),
        provider_fallbacks: vec![],
    });
    profile.sub_recipes = Some(vec![SubRecipeRef {
        name: "helper".into(),
        path: "recipes/helper.yaml".into(),
        description: Some("Helper".into()),
    }]);
    profile.parameters = Some(vec![ParameterRef {
        key: "task".into(),
        input_type: "select".into(),
        requirement: "user_prompt".into(),
        description: "Task selector".into(),
        default: Some("lint".into()),
    }]);

    let profile = Arc::new(profile);
    let barrier = Arc::new(Barrier::new(8));

    thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..8 {
            let profile = Arc::clone(&profile);
            let barrier = Arc::clone(&barrier);
            handles.push(scope.spawn(move || {
                barrier.wait();
                let recipe = profile_to_recipe(&profile);
                let round_trip = recipe_to_profile(&recipe);
                (
                    recipe.title,
                    recipe.extensions.as_ref().map(Vec::len),
                    recipe.parameters.as_ref().map(Vec::len),
                    round_trip.title,
                    round_trip.extensions.len(),
                    round_trip.parameters.as_ref().map(Vec::len),
                    round_trip
                        .settings
                        .as_ref()
                        .and_then(|settings| settings.max_retries),
                )
            }));
        }

        for handle in handles {
            let result = handle.join().unwrap();
            assert_eq!(result.0, "concurrent");
            assert_eq!(result.1, Some(2));
            assert_eq!(result.2, Some(1));
            assert_eq!(result.3, "concurrent");
            assert_eq!(result.4, 2);
            assert_eq!(result.5, Some(1));
            assert_eq!(result.6, Some(2));
        }
    });
}
