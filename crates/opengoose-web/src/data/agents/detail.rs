use anyhow::Result;
use opengoose_profiles::AgentProfile;

use super::catalog::ProfileCatalogEntry;
use crate::data::utils::preview;
use crate::data::views::{AgentDetailView, ExtensionRow, SettingRow};

pub(super) fn build_agent_detail(entry: &ProfileCatalogEntry) -> Result<AgentDetailView> {
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
        settings: profile_settings(&entry.profile),
        activities: entry.profile.activities.clone().unwrap_or_default(),
        skills: entry.profile.skills.clone(),
        extensions: build_extensions(&entry.profile),
        yaml: entry.profile.to_yaml()?,
    })
}

pub(super) fn capability_line(profile: &AgentProfile) -> String {
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

pub(super) fn profile_settings(profile: &AgentProfile) -> Vec<SettingRow> {
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

fn build_extensions(profile: &AgentProfile) -> Vec<ExtensionRow> {
    profile
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
        .collect()
}
