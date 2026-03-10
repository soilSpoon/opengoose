use std::sync::Arc;

use anyhow::{Context, Result};
use opengoose_persistence::{
    Database, OrchestrationRun, OrchestrationStore, SessionStore, SessionSummary,
};
use opengoose_profiles::{AgentProfile, ProfileStore, all_defaults as default_profiles};
use opengoose_teams::{TeamDefinition, TeamStore, all_defaults as default_teams};
use opengoose_types::SessionKey;
use urlencoding::encode;

use super::runs::mock_runs;
use super::sessions::mock_sessions;
use crate::data::utils::{choose_selected_name, platform_tone, preview, progress_label, run_tone};
use crate::data::views::{
    AgentDetailView, AgentListItem, AgentRecentRunView, AgentSessionView, AgentsPageView,
    ExtensionRow, SettingRow,
};

#[derive(Clone)]
struct ProfileCatalogEntry {
    profile: AgentProfile,
    source_label: String,
    is_live: bool,
}

struct AgentRuntimeCatalog {
    teams: Vec<TeamDefinition>,
    runs: Vec<OrchestrationRun>,
    sessions: Vec<SessionSummary>,
}

/// Load the agents page view-model, optionally selecting an agent by name.
pub fn load_agents_page(db: Arc<Database>, selected: Option<String>) -> Result<AgentsPageView> {
    let agents = load_profiles_catalog()?;
    let using_defaults = agents.iter().all(|profile| !profile.is_live);
    let runtime = load_agent_runtime_catalog(db, using_defaults)?;
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
                page_url: format!("/agents?agent={}", encode(&entry.profile.title)),
                active: entry.profile.title == selected_name,
            })
            .collect(),
        selected: build_agent_detail(
            agents
                .iter()
                .find(|entry| entry.profile.title == selected_name)
                .context("selected agent missing")?,
            &runtime,
        )?,
    })
}

/// Load the detail panel for a single agent profile.
pub fn load_agent_detail(db: Arc<Database>, selected: Option<String>) -> Result<AgentDetailView> {
    Ok(load_agents_page(db, selected)?.selected)
}

/// Load an exact agent profile detail for standalone pages and JSON detail endpoints.
pub fn load_agent_detail_exact(db: Arc<Database>, name: &str) -> Result<Option<AgentDetailView>> {
    let agents = load_profiles_catalog()?;
    let using_defaults = agents.iter().all(|profile| !profile.is_live);
    let runtime = load_agent_runtime_catalog(db, using_defaults)?;

    agents
        .iter()
        .find(|entry| entry.profile.title == name)
        .map(|entry| build_agent_detail(entry, &runtime))
        .transpose()
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

fn load_agent_runtime_catalog(
    db: Arc<Database>,
    using_defaults: bool,
) -> Result<AgentRuntimeCatalog> {
    let live_teams = load_live_teams()?;
    let live_runs = OrchestrationStore::new(db.clone()).list_runs(None, 200)?;
    let live_sessions = SessionStore::new(db).list_sessions(24)?;
    let runtime_preview =
        using_defaults && live_teams.is_empty() && live_runs.is_empty() && live_sessions.is_empty();

    Ok(AgentRuntimeCatalog {
        teams: if runtime_preview {
            default_teams()
        } else {
            live_teams
        },
        runs: if runtime_preview {
            mock_runs()
        } else {
            live_runs
        },
        sessions: if runtime_preview {
            mock_sessions()
                .into_iter()
                .map(|record| record.summary)
                .collect()
        } else {
            live_sessions
        },
    })
}

fn load_live_teams() -> Result<Vec<TeamDefinition>> {
    let store = TeamStore::new()?;
    let names = store.list()?;
    names
        .into_iter()
        .map(|name| store.get(&name).map_err(Into::into))
        .collect()
}

fn build_agent_detail(
    entry: &ProfileCatalogEntry,
    runtime: &AgentRuntimeCatalog,
) -> Result<AgentDetailView> {
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
    let team_names = team_memberships(&runtime.teams, &entry.profile.title);
    let recent_runs = recent_runs_for_profile(&runtime.runs, &team_names);
    let connected_sessions = connected_sessions_for_profile(&runtime.sessions, &team_names);

    Ok(AgentDetailView {
        title: entry.profile.title.clone(),
        subtitle: entry
            .profile
            .description
            .clone()
            .unwrap_or_else(|| "No profile description provided.".into()),
        source_label: entry.source_label.clone(),
        detail_page_url: format!("/agents/{}", encode(&entry.profile.title)),
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
        recent_runs,
        connected_sessions,
        runtime_empty_hint:
            "No related orchestration runs or active sessions have been recorded for this profile yet."
                .into(),
        yaml: entry.profile.to_yaml()?,
    })
}

fn team_memberships(teams: &[TeamDefinition], profile_title: &str) -> Vec<String> {
    teams
        .iter()
        .filter(|team| {
            team.agents
                .iter()
                .any(|agent| agent.profile == profile_title)
        })
        .map(|team| team.title.clone())
        .collect()
}

fn recent_runs_for_profile(
    runs: &[OrchestrationRun],
    team_names: &[String],
) -> Vec<AgentRecentRunView> {
    runs.iter()
        .filter(|run| {
            team_names
                .iter()
                .any(|team_name| team_name == &run.team_name)
        })
        .take(6)
        .map(|run| AgentRecentRunView {
            title: run.team_name.clone(),
            detail: format!(
                "Run {} · {} workflow · {}",
                run.team_run_id,
                run.workflow,
                progress_label(run)
            ),
            updated_at: run.updated_at.clone(),
            status_label: run.status.as_str().to_uppercase(),
            status_tone: run_tone(&run.status),
            page_url: format!("/runs/{}", encode(&run.team_run_id)),
        })
        .collect()
}

fn connected_sessions_for_profile(
    sessions: &[SessionSummary],
    team_names: &[String],
) -> Vec<AgentSessionView> {
    sessions
        .iter()
        .filter(|session| {
            session
                .active_team
                .as_ref()
                .map(|team_name| team_names.iter().any(|candidate| candidate == team_name))
                .unwrap_or(false)
        })
        .take(6)
        .map(|session| {
            let parsed = SessionKey::from_stable_id(&session.session_key);
            AgentSessionView {
                title: match &parsed.namespace {
                    Some(namespace) => format!("{namespace} / {}", parsed.channel_id),
                    None => parsed.channel_id.clone(),
                },
                detail: session
                    .active_team
                    .clone()
                    .map(|team| format!("{team} active"))
                    .unwrap_or_else(|| "No active team".into()),
                updated_at: session.updated_at.clone(),
                badge: parsed.platform.as_str().to_uppercase(),
                badge_tone: platform_tone(parsed.platform.as_str()),
                page_url: format!("/sessions?session={}", encode(&session.session_key)),
            }
        })
        .collect()
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
