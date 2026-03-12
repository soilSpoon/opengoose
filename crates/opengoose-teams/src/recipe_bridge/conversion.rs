//! Profile-to-recipe and recipe-to-profile conversion logic.

use goose::agents::RetryConfig;
use goose::agents::types::SuccessCheck;
use goose::recipe::{
    Recipe, RecipeParameter, RecipeParameterInputType, RecipeParameterRequirement, Response,
    Settings, SubRecipe,
};

use opengoose_profiles::{AgentProfile, ParameterRef, ProfileSettings, SubRecipeRef};

use super::extensions::{config_to_ext_ref, ext_ref_to_config};

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

    let extensions = if profile.extensions.is_empty() {
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
            message_retention_days: None,
            event_retention_days: None,
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
            provider_fallbacks: vec![],
        })
    } else {
        None
    };

    let extensions = recipe
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
        skills: vec![],
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
