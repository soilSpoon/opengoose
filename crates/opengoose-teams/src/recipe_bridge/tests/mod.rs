use goose::recipe::Recipe;

use opengoose_profiles::AgentProfile;

mod command;
mod concurrency;
mod conversion;
mod extensions;
mod fallback;
mod round_trip;
mod stream;

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
