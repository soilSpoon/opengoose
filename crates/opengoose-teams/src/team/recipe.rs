use goose::agents::extension::ExtensionConfig;
use goose::recipe::{Recipe, SubRecipe};
use opengoose_profiles::ProfileStore;

use super::types::{OrchestrationPattern, TeamDefinition};

impl TeamDefinition {
    /// Convert this team into a Goose `Recipe` for Goose CLI compatibility.
    ///
    /// Each team member becomes a sub-recipe (Summon extension), and the
    /// orchestration logic is described in the recipe instructions. This
    /// allows the team to be executed via `goose run --recipe team.yaml`.
    pub fn to_recipe(&self, profile_store: &ProfileStore) -> Recipe {
        let sub_recipes: Vec<SubRecipe> = self
            .agents
            .iter()
            .map(|a| SubRecipe {
                name: a.profile.clone(),
                path: profile_store.profile_path(&a.profile),
                values: None,
                sequential_when_repeated: matches!(self.workflow, OrchestrationPattern::Chain),
                description: a.role.clone(),
            })
            .collect();

        let instructions = self.generate_orchestration_instructions();

        Recipe {
            version: self.version.clone(),
            title: self.title.clone(),
            description: self.description.clone().unwrap_or_default(),
            instructions: Some(instructions),
            prompt: None,
            extensions: Some(vec![ExtensionConfig::Platform {
                name: "summon".into(),
                description: String::new(),
                display_name: None,
                bundled: None,
                available_tools: vec![],
            }]),
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            sub_recipes: Some(sub_recipes),
            retry: None,
        }
    }

    pub(crate) fn generate_orchestration_instructions(&self) -> String {
        match self.workflow {
            OrchestrationPattern::Chain => {
                let steps: Vec<String> = self
                    .agents
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        format!(
                            "{}. Delegate to '{}'{}",
                            i + 1,
                            a.profile,
                            a.role
                                .as_ref()
                                .map(|r| format!(" ({r})"))
                                .unwrap_or_default()
                        )
                    })
                    .collect();
                format!(
                    "Execute the following agents in sequence, \
                     passing each output as input to the next:\n{}",
                    steps.join("\n")
                )
            }
            OrchestrationPattern::FanOut => {
                let agents: Vec<String> = self
                    .agents
                    .iter()
                    .map(|a| format!("- '{}'", a.profile))
                    .collect();
                format!(
                    "Delegate to ALL of the following agents simultaneously (async), \
                     then synthesize their results:\n{}",
                    agents.join("\n")
                )
            }
            OrchestrationPattern::Router => {
                let agents: Vec<String> = self
                    .agents
                    .iter()
                    .map(|a| {
                        format!(
                            "- '{}'{}",
                            a.profile,
                            a.role
                                .as_ref()
                                .map(|r| format!(": {r}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect();
                format!(
                    "Analyze the input and delegate to the most appropriate agent:\n{}",
                    agents.join("\n")
                )
            }
        }
    }
}
