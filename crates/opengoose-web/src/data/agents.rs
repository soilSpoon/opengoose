use anyhow::{Context, Result};
use opengoose_profiles::{AgentProfile, ProfileStore, all_defaults as default_profiles};
use urlencoding::encode;

use crate::data::utils::{choose_selected_name, preview, source_badge};
use crate::data::views::{
    AgentDetailView, AgentListItem, AgentsPageView, ExtensionRow, SettingRow,
};

#[derive(Clone)]
struct ProfileCatalogEntry {
    profile: AgentProfile,
    source_label: String,
    is_live: bool,
}

/// Load the agents page view-model, optionally selecting an agent by name.
pub fn load_agents_page(selected: Option<String>) -> Result<AgentsPageView> {
    let agents = load_profiles_catalog()?;
    let using_defaults = agents.iter().all(|profile| !profile.is_live);
    let selected_name = choose_selected_name(
        agents
            .iter()
            .map(|item| item.profile.title.clone())
            .collect(),
        selected,
    );

    Ok(AgentsPageView {
        mode_label: if using_defaults {
            "Bundled defaults".into()
        } else {
            "Installed catalog".into()
        },
        mode_tone: if using_defaults { "neutral" } else { "success" },
        agents: agents
            .iter()
            .map(|entry| AgentListItem {
                title: entry.profile.title.clone(),
                subtitle: entry
                    .profile
                    .description
                    .clone()
                    .unwrap_or_else(|| "No profile description provided.".into()),
                capability: capability_line(&entry.profile),
                source_label: entry.source_label.clone(),
                source_badge: source_badge(&entry.source_label),
                page_url: format!("/agents?agent={}", encode(&entry.profile.title)),
                active: entry.profile.title == selected_name,
            })
            .collect(),
        selected: build_agent_detail(
            agents
                .iter()
                .find(|entry| entry.profile.title == selected_name)
                .context("selected agent missing")?,
        )?,
    })
}

fn load_profiles_catalog() -> Result<Vec<ProfileCatalogEntry>> {
    let store = ProfileStore::new()?;
    let names = store.list()?;
    if names.is_empty() {
        return Ok(default_profiles()
            .into_iter()
            .map(|profile| ProfileCatalogEntry {
                profile,
                source_label: "Bundled default".into(),
                is_live: false,
            })
            .collect());
    }

    names
        .into_iter()
        .map(|name| {
            let profile = store.get(&name)?;
            Ok(ProfileCatalogEntry {
                profile,
                source_label: store.profile_path(&name),
                is_live: true,
            })
        })
        .collect()
}

fn build_agent_detail(entry: &ProfileCatalogEntry) -> Result<AgentDetailView> {
    let settings = profile_settings(&entry.profile);
    let extensions = entry
        .profile
        .extensions
        .iter()
        .map(|extension| ExtensionRow {
            name: extension.name.clone(),
            kind: extension.ext_type.clone(),
            summary: extension
                .cmd
                .clone()
                .or_else(|| extension.uri.clone())
                .or_else(|| extension.code.as_ref().map(|_| "inline python".into()))
                .unwrap_or_else(|| "No runtime configuration".into()),
        })
        .collect();

    Ok(AgentDetailView {
        title: entry.profile.title.clone(),
        subtitle: entry
            .profile
            .description
            .clone()
            .unwrap_or_else(|| "No profile description provided.".into()),
        source_label: entry.source_label.clone(),
        instructions_preview: preview(
            entry
                .profile
                .instructions
                .as_deref()
                .or(entry.profile.prompt.as_deref())
                .unwrap_or("No instructions or prompt configured."),
            420,
        ),
        settings,
        activities: entry.profile.activities.clone().unwrap_or_default(),
        skills: entry.profile.skills.clone(),
        extensions,
        yaml: entry.profile.to_yaml()?,
    })
}

fn capability_line(profile: &AgentProfile) -> String {
    let provider = profile
        .settings
        .as_ref()
        .and_then(|settings| settings.goose_provider.clone())
        .unwrap_or_else(|| "provider unset".into());
    let model = profile
        .settings
        .as_ref()
        .and_then(|settings| settings.goose_model.clone())
        .unwrap_or_else(|| "model unset".into());
    format!("{provider} / {model}")
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use opengoose_profiles::{AgentProfile, ProfileSettings};

    use super::*;

    fn minimal_profile(title: &str) -> AgentProfile {
        AgentProfile {
            version: "1.0.0".into(),
            title: title.into(),
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
        }
    }

    // --- capability_line ---

    #[test]
    fn capability_line_with_provider_and_model() {
        let mut profile = minimal_profile("test-agent");
        profile.settings = Some(ProfileSettings {
            goose_provider: Some("anthropic".into()),
            goose_model: Some("claude-4".into()),
            ..ProfileSettings::default()
        });
        let line = capability_line(&profile);
        assert_eq!(line, "anthropic / claude-4");
    }

    #[test]
    fn capability_line_no_settings_shows_unset() {
        let profile = minimal_profile("test-agent");
        let line = capability_line(&profile);
        assert_eq!(line, "provider unset / model unset");
    }

    #[test]
    fn capability_line_provider_only() {
        let mut profile = minimal_profile("test-agent");
        profile.settings = Some(ProfileSettings {
            goose_provider: Some("openai".into()),
            ..ProfileSettings::default()
        });
        let line = capability_line(&profile);
        assert!(line.starts_with("openai / "));
        assert!(line.contains("model unset"));
    }

    // --- profile_settings ---

    #[test]
    fn profile_settings_no_settings_returns_placeholder_row() {
        let profile = minimal_profile("test-agent");
        let rows = profile_settings(&profile);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Settings");
        assert!(rows[0].value.contains("No explicit settings"));
    }

    #[test]
    fn profile_settings_with_provider_and_model() {
        let mut profile = minimal_profile("test-agent");
        profile.settings = Some(ProfileSettings {
            goose_provider: Some("anthropic".into()),
            goose_model: Some("claude-4".into()),
            ..ProfileSettings::default()
        });
        let rows = profile_settings(&profile);
        let labels: Vec<_> = rows.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"Provider"));
        assert!(labels.contains(&"Model"));
    }

    #[test]
    fn profile_settings_with_temperature() {
        let mut profile = minimal_profile("test-agent");
        profile.settings = Some(ProfileSettings {
            temperature: Some(0.7),
            ..ProfileSettings::default()
        });
        let rows = profile_settings(&profile);
        let temp_row = rows.iter().find(|r| r.label == "Temperature");
        assert!(temp_row.is_some());
        assert!(temp_row.unwrap().value.contains("0.7"));
    }

    #[test]
    fn profile_settings_with_max_turns_and_retries() {
        let mut profile = minimal_profile("test-agent");
        profile.settings = Some(ProfileSettings {
            max_turns: Some(10),
            max_retries: Some(3),
            ..ProfileSettings::default()
        });
        let rows = profile_settings(&profile);
        let labels: Vec<_> = rows.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"Max turns"));
        assert!(labels.contains(&"Retries"));
    }

    #[test]
    fn profile_settings_empty_settings_block_returns_placeholder() {
        let mut profile = minimal_profile("test-agent");
        profile.settings = Some(ProfileSettings::default());
        let rows = profile_settings(&profile);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Settings");
    }
}

fn profile_settings(profile: &AgentProfile) -> Vec<SettingRow> {
    let mut rows = Vec::new();
    if let Some(settings) = &profile.settings {
        if let Some(provider) = &settings.goose_provider {
            rows.push(SettingRow {
                label: "Provider".into(),
                value: provider.clone(),
            });
        }
        if let Some(model) = &settings.goose_model {
            rows.push(SettingRow {
                label: "Model".into(),
                value: model.clone(),
            });
        }
        if let Some(temperature) = settings.temperature {
            rows.push(SettingRow {
                label: "Temperature".into(),
                value: temperature.to_string(),
            });
        }
        if let Some(max_turns) = settings.max_turns {
            rows.push(SettingRow {
                label: "Max turns".into(),
                value: max_turns.to_string(),
            });
        }
        if let Some(max_retries) = settings.max_retries {
            rows.push(SettingRow {
                label: "Retries".into(),
                value: max_retries.to_string(),
            });
        }
    }
    if rows.is_empty() {
        rows.push(SettingRow {
            label: "Settings".into(),
            value: "No explicit settings block".into(),
        });
    }
    rows
}
