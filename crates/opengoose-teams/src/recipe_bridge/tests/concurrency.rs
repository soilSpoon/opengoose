use std::collections::HashMap;
use std::sync::{Arc, Barrier};
use std::thread;

use opengoose_profiles::{ExtensionRef, ParameterRef, ProfileSettings, SubRecipeRef};

use super::super::{profile_to_recipe, recipe_to_profile};
use super::empty_profile;

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
