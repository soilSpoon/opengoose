//! Bidirectional conversion between `AgentProfile` and Goose `Recipe`.
//!
//! This allows OpenGoose profiles to be used as Goose recipes and vice versa,
//! enabling interoperability with the broader Goose ecosystem (sub-recipes,
//! Summon extension, recipe sharing).

use std::collections::HashMap;

use goose::agents::RetryConfig;
use goose::agents::extension::{Envs, ExtensionConfig};
use goose::agents::types::SuccessCheck;
use goose::recipe::{
    Recipe, RecipeParameter, RecipeParameterInputType, RecipeParameterRequirement, Response,
    Settings, SubRecipe,
};

use opengoose_profiles::{AgentProfile, ExtensionRef, ParameterRef, ProfileSettings, SubRecipeRef};

/// Build a Goose `RetryConfig` from `ProfileSettings`.
///
/// Returns `None` if `max_retries` is not set in the profile settings.
pub fn settings_to_retry_config(settings: &ProfileSettings) -> Option<RetryConfig> {
    let max_retries = settings.max_retries?;
    let checks = settings
        .retry_checks
        .iter()
        .map(|cmd| SuccessCheck::Shell {
            command: cmd.clone(),
        })
        .collect();
    Some(RetryConfig {
        max_retries,
        checks,
        on_failure: settings.on_failure.clone(),
        timeout_seconds: None,
        on_failure_timeout_seconds: None,
    })
}

/// Convert an `AgentProfile` into a Goose `Recipe`.
pub fn profile_to_recipe(profile: &AgentProfile) -> Recipe {
    let settings = profile.settings.as_ref();

    let recipe_settings = settings.map(|s| Settings {
        goose_provider: s.goose_provider.clone(),
        goose_model: s.goose_model.clone(),
        temperature: s.temperature.map(|t| t as f32),
        max_turns: s.max_turns.map(|t| t as usize),
    });

    let retry = settings.and_then(settings_to_retry_config);

    let extensions: Option<Vec<ExtensionConfig>> = if profile.extensions.is_empty() {
        None
    } else {
        Some(
            profile
                .extensions
                .iter()
                .filter_map(ext_ref_to_config)
                .collect(),
        )
    };

    let response = profile.response.as_ref().map(|schema| Response {
        json_schema: Some(schema.clone()),
    });

    let sub_recipes = profile.sub_recipes.as_ref().map(|subs| {
        subs.iter()
            .map(|s| SubRecipe {
                name: s.name.clone(),
                path: s.path.clone(),
                values: None,
                sequential_when_repeated: false,
                description: s.description.clone(),
            })
            .collect()
    });

    let parameters = profile.parameters.as_ref().map(|params| {
        params
            .iter()
            .map(|p| RecipeParameter {
                key: p.key.clone(),
                input_type: parse_input_type(&p.input_type),
                requirement: parse_requirement(&p.requirement),
                description: p.description.clone(),
                default: p.default.clone(),
                options: None,
            })
            .collect()
    });

    Recipe {
        version: profile.version.clone(),
        title: profile.title.clone(),
        description: profile.description.clone().unwrap_or_default(),
        instructions: profile.instructions.clone(),
        prompt: profile.prompt.clone(),
        extensions,
        settings: recipe_settings,
        activities: profile.activities.clone(),
        author: None,
        parameters,
        response,
        sub_recipes,
        retry,
    }
}

/// Convert a Goose `Recipe` into an `AgentProfile`.
pub fn recipe_to_profile(recipe: &Recipe) -> AgentProfile {
    let settings = recipe.settings.as_ref();

    let profile_settings = if settings.is_some() || recipe.retry.is_some() {
        Some(ProfileSettings {
            goose_provider: settings.and_then(|s| s.goose_provider.clone()),
            goose_model: settings.and_then(|s| s.goose_model.clone()),
            temperature: settings.and_then(|s| s.temperature).map(|t| t as f64),
            max_turns: settings.and_then(|s| s.max_turns).map(|t| t as u32),
            max_retries: recipe.retry.as_ref().map(|r| r.max_retries),
            retry_checks: recipe
                .retry
                .as_ref()
                .map(|r| {
                    r.checks
                        .iter()
                        .map(|c| match c {
                            SuccessCheck::Shell { command } => command.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            on_failure: recipe.retry.as_ref().and_then(|r| r.on_failure.clone()),
        })
    } else {
        None
    };

    let extensions: Vec<ExtensionRef> = recipe
        .extensions
        .as_ref()
        .map(|exts| exts.iter().filter_map(config_to_ext_ref).collect())
        .unwrap_or_default();

    let response = recipe.response.as_ref().and_then(|r| r.json_schema.clone());

    let sub_recipes = recipe.sub_recipes.as_ref().map(|subs| {
        subs.iter()
            .map(|s| SubRecipeRef {
                name: s.name.clone(),
                path: s.path.clone(),
                description: s.description.clone(),
            })
            .collect()
    });

    let parameters = recipe.parameters.as_ref().map(|params| {
        params
            .iter()
            .map(|p| ParameterRef {
                key: p.key.clone(),
                input_type: format_input_type(&p.input_type),
                requirement: format_requirement(&p.requirement),
                description: p.description.clone(),
                default: p.default.clone(),
            })
            .collect()
    });

    AgentProfile {
        version: recipe.version.clone(),
        title: recipe.title.clone(),
        description: if recipe.description.is_empty() {
            None
        } else {
            Some(recipe.description.clone())
        },
        instructions: recipe.instructions.clone(),
        prompt: recipe.prompt.clone(),
        extensions,
        settings: profile_settings,
        activities: recipe.activities.clone(),
        response,
        sub_recipes,
        parameters,
    }
}

fn parse_input_type(s: &str) -> RecipeParameterInputType {
    match s {
        "number" => RecipeParameterInputType::Number,
        "boolean" => RecipeParameterInputType::Boolean,
        "date" => RecipeParameterInputType::Date,
        "file" => RecipeParameterInputType::File,
        "select" => RecipeParameterInputType::Select,
        _ => RecipeParameterInputType::String,
    }
}

fn format_input_type(t: &RecipeParameterInputType) -> String {
    match t {
        RecipeParameterInputType::String => "string",
        RecipeParameterInputType::Number => "number",
        RecipeParameterInputType::Boolean => "boolean",
        RecipeParameterInputType::Date => "date",
        RecipeParameterInputType::File => "file",
        RecipeParameterInputType::Select => "select",
    }
    .to_string()
}

fn parse_requirement(s: &str) -> RecipeParameterRequirement {
    match s {
        "required" => RecipeParameterRequirement::Required,
        "user_prompt" => RecipeParameterRequirement::UserPrompt,
        _ => RecipeParameterRequirement::Optional,
    }
}

fn format_requirement(r: &RecipeParameterRequirement) -> String {
    match r {
        RecipeParameterRequirement::Required => "required",
        RecipeParameterRequirement::Optional => "optional",
        RecipeParameterRequirement::UserPrompt => "user_prompt",
    }
    .to_string()
}

/// Convert an `ExtensionRef` into a Goose `ExtensionConfig`.
///
/// Returns `None` for unsupported extension types or when required fields
/// (e.g. `cmd` for stdio, `uri` for streamable_http) are missing.
pub fn ext_ref_to_config(ext: &ExtensionRef) -> Option<ExtensionConfig> {
    match ext.ext_type.as_str() {
        "builtin" => Some(ExtensionConfig::Builtin {
            name: ext.name.clone(),
            description: String::new(),
            display_name: None,
            timeout: ext.timeout,
            bundled: Some(true),
            available_tools: vec![],
        }),
        "stdio" => {
            let cmd = ext.cmd.as_ref()?.clone();
            Some(ExtensionConfig::Stdio {
                name: ext.name.clone(),
                description: String::new(),
                cmd,
                args: ext.args.clone(),
                envs: Envs::new(ext.envs.clone()),
                env_keys: ext.env_keys.clone(),
                timeout: ext.timeout,
                bundled: None,
                available_tools: vec![],
            })
        }
        "streamable_http" => {
            let uri = ext.uri.as_ref()?.clone();
            Some(ExtensionConfig::StreamableHttp {
                name: ext.name.clone(),
                description: String::new(),
                uri,
                envs: Envs::new(ext.envs.clone()),
                env_keys: ext.env_keys.clone(),
                headers: HashMap::new(),
                timeout: ext.timeout,
                bundled: None,
                available_tools: vec![],
            })
        }
        "platform" => Some(ExtensionConfig::Platform {
            name: ext.name.clone(),
            description: String::new(),
            display_name: None,
            bundled: None,
            available_tools: vec![],
        }),
        "inline_python" => {
            let code = ext.code.as_ref()?.clone();
            Some(ExtensionConfig::InlinePython {
                name: ext.name.clone(),
                description: String::new(),
                code,
                timeout: ext.timeout,
                dependencies: ext.dependencies.clone(),
                available_tools: vec![],
            })
        }
        _ => None,
    }
}

fn config_to_ext_ref(config: &ExtensionConfig) -> Option<ExtensionRef> {
    match config {
        ExtensionConfig::Builtin { name, timeout, .. } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "builtin".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: *timeout,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        }),
        ExtensionConfig::Stdio {
            name,
            cmd,
            args,
            envs,
            env_keys,
            timeout,
            ..
        } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "stdio".into(),
            cmd: Some(cmd.clone()),
            args: args.clone(),
            uri: None,
            timeout: *timeout,
            envs: envs.get_env(),
            env_keys: env_keys.clone(),
            code: None,
            dependencies: None,
        }),
        ExtensionConfig::StreamableHttp {
            name,
            uri,
            envs,
            env_keys,
            timeout,
            ..
        } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "streamable_http".into(),
            cmd: None,
            args: vec![],
            uri: Some(uri.clone()),
            timeout: *timeout,
            envs: envs.get_env(),
            env_keys: env_keys.clone(),
            code: None,
            dependencies: None,
        }),
        ExtensionConfig::Platform { name, .. } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "platform".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        }),
        ExtensionConfig::InlinePython {
            name,
            code,
            timeout,
            dependencies,
            ..
        } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "inline_python".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: *timeout,
            envs: HashMap::new(),
            env_keys: vec![],
            code: Some(code.clone()),
            dependencies: dependencies.clone(),
        }),
        // Sse and Frontend are not mapped to profiles
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            settings: Some(ProfileSettings {
                goose_provider: Some("anthropic".into()),
                goose_model: Some("claude-sonnet-4-20250514".into()),
                temperature: Some(0.7),
                max_turns: Some(10),
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
            other => panic!("expected InlinePython, got {:?}", other),
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
            settings: None,
            activities: None,
            response: None,
            sub_recipes: None,
            parameters: None,
        };

        let recipe = profile_to_recipe(&profile);
        match &recipe.extensions.as_ref().unwrap()[0] {
            ExtensionConfig::Platform { name, .. } => assert_eq!(name, "summon"),
            other => panic!("expected Platform, got {:?}", other),
        }

        let back = recipe_to_profile(&recipe);
        assert_eq!(back.extensions[0].ext_type, "platform");
        assert_eq!(back.extensions[0].name, "summon");
    }
}
