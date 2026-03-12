use crate::data::utils::{preview, source_badge};
use crate::data::views::{MetaRow, Notice, PluginDetailView, PluginListItem};

use super::PluginStatusFilter;
use super::catalog::plugins_page_url;
use super::state::{PluginState, build_plugin_search_text};

pub(super) fn build_plugin_list_item(
    state: &PluginState,
    selected_name: Option<&str>,
    filter: PluginStatusFilter,
) -> PluginListItem {
    let plugin = &state.plugin;
    let subtitle = match plugin.author.as_deref() {
        Some(author) if !author.trim().is_empty() => format!("v{} · {}", plugin.version, author),
        _ => format!("v{}", plugin.version),
    };

    PluginListItem {
        title: plugin.name.clone(),
        subtitle,
        preview: plugin
            .description
            .as_deref()
            .map(|description| preview(description, 84))
            .unwrap_or_else(|| "No plugin description provided.".into()),
        status_detail: preview(&state.status_summary, 92),
        search_text: build_plugin_search_text(state),
        source_label: plugin.source_path.clone(),
        source_badge: source_badge(&plugin.source_path),
        status_label: state.status_label.clone(),
        status_tone: state.status_tone,
        page_url: plugins_page_url(Some(&plugin.name), filter),
        active: selected_name == Some(plugin.name.as_str()),
    }
}

pub(super) fn build_plugin_detail(
    state: &PluginState,
    notice: Option<Notice>,
    install_source_path: String,
) -> PluginDetailView {
    let plugin = &state.plugin;
    let snapshot = &state.snapshot;
    let capabilities = snapshot.capabilities.clone();

    PluginDetailView {
        title: plugin.name.clone(),
        subtitle: plugin.description.clone().unwrap_or_else(|| {
            "This plugin is installed locally and ready for operator review.".into()
        }),
        source_label: plugin.source_path.clone(),
        status_label: state.status_label.clone(),
        status_tone: state.status_tone,
        lifecycle_label: state.lifecycle_label.clone(),
        lifecycle_tone: state.lifecycle_tone,
        runtime_label: state.runtime_label.clone(),
        runtime_tone: state.runtime_tone,
        status_summary: state.status_summary.clone(),
        runtime_note: snapshot.runtime_note.clone(),
        meta: vec![
            MetaRow {
                label: "Version".into(),
                value: plugin.version.clone(),
            },
            MetaRow {
                label: "Author".into(),
                value: plugin.author.clone().unwrap_or_else(|| "Unknown".into()),
            },
            MetaRow {
                label: "Lifecycle".into(),
                value: state.lifecycle_label.clone(),
            },
            MetaRow {
                label: "Runtime".into(),
                value: state.runtime_label.clone(),
            },
            MetaRow {
                label: "Registered skills".into(),
                value: snapshot.registered_skills.len().to_string(),
            },
            MetaRow {
                label: "Missing skills".into(),
                value: snapshot.missing_skills.len().to_string(),
            },
            MetaRow {
                label: "Installed".into(),
                value: plugin.created_at.clone(),
            },
            MetaRow {
                label: "Updated".into(),
                value: plugin.updated_at.clone(),
            },
        ],
        capabilities,
        capabilities_hint: "No capabilities declared in plugin.toml.".into(),
        registered_skills: snapshot.registered_skills.clone(),
        missing_skills: snapshot.missing_skills.clone(),
        notice,
        install_source_path,
        toggle_label: if plugin.enabled {
            "Disable plugin".into()
        } else {
            "Enable plugin".into()
        },
        delete_label: plugin.name.clone(),
        is_placeholder: false,
    }
}

pub(super) fn placeholder_plugin_detail(
    notice: Option<Notice>,
    install_source_path: String,
    filter: PluginStatusFilter,
    total_plugins: usize,
) -> PluginDetailView {
    let (title, subtitle, status_label, status_summary, capabilities_hint) = if total_plugins == 0 {
        (
            "No plugins installed".to_string(),
            "Install a plugin directory with a plugin.toml manifest to start managing plugin lifecycle from the dashboard.".to_string(),
            "Awaiting install".to_string(),
            "Install a plugin to inspect runtime readiness.".to_string(),
            "Installed plugin capabilities will appear here.".to_string(),
        )
    } else {
        (
            format!("No {} plugins", filter.label().to_lowercase()),
            format!(
                "{} installed plugin(s) exist outside the current status filter.",
                total_plugins
            ),
            "Adjust filter".to_string(),
            "Choose another filter or clear the filter to inspect all plugins.".to_string(),
            "Visible plugin capabilities will appear here once a filter matches.".to_string(),
        )
    };

    PluginDetailView {
        title,
        subtitle,
        source_label: "Local plugin registry".into(),
        status_label,
        status_tone: "neutral",
        lifecycle_label: "Awaiting selection".into(),
        lifecycle_tone: "neutral",
        runtime_label: "No runtime data".into(),
        runtime_tone: "neutral",
        status_summary,
        runtime_note: None,
        meta: vec![],
        capabilities: vec![],
        capabilities_hint,
        registered_skills: vec![],
        missing_skills: vec![],
        notice,
        install_source_path,
        toggle_label: String::new(),
        delete_label: String::new(),
        is_placeholder: true,
    }
}
