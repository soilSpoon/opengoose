use std::sync::Arc;

use anyhow::Result;
use opengoose_core::plugins::{install_plugin, remove_plugin, set_plugin_enabled};
use opengoose_persistence::{Database, Plugin, PluginStore};
use opengoose_profiles::SkillStore;
use opengoose_teams::plugin::plugin_status_snapshot;
use opengoose_types::PluginStatusSnapshot;
use urlencoding::encode;

use crate::data::utils::{preview, source_badge};
use crate::data::views::{
    MetaRow, Notice, PluginDetailView, PluginFilterItem, PluginListItem, PluginsPageView,
};

const MAX_PLUGIN_PATH_BYTES: usize = 4096;

pub struct PluginInstallInput {
    pub source_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatusFilter {
    All,
    Operational,
    Attention,
    Disabled,
}

impl PluginStatusFilter {
    pub fn from_query(value: Option<&str>) -> Self {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            Some("operational") => Self::Operational,
            Some("attention") => Self::Attention,
            Some("disabled") => Self::Disabled,
            _ => Self::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Operational => "Operational",
            Self::Attention => "Attention",
            Self::Disabled => "Disabled",
        }
    }

    fn tone(self) -> &'static str {
        match self {
            Self::All => "neutral",
            Self::Operational => "success",
            Self::Attention => "amber",
            Self::Disabled => "neutral",
        }
    }

    fn query_value(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Operational => Some("operational"),
            Self::Attention => Some("attention"),
            Self::Disabled => Some("disabled"),
        }
    }

    fn matches(self, bucket: PluginStatusBucket) -> bool {
        match self {
            Self::All => true,
            Self::Operational => bucket == PluginStatusBucket::Operational,
            Self::Attention => bucket == PluginStatusBucket::Attention,
            Self::Disabled => bucket == PluginStatusBucket::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginStatusBucket {
    Operational,
    Attention,
    Disabled,
}

#[derive(Debug, Clone, Copy, Default)]
struct PluginStatusCounts {
    operational: usize,
    attention: usize,
    disabled: usize,
}

#[derive(Clone)]
struct PluginState {
    plugin: Plugin,
    snapshot: PluginStatusSnapshot,
    bucket: PluginStatusBucket,
    status_label: String,
    status_tone: &'static str,
    lifecycle_label: String,
    lifecycle_tone: &'static str,
    runtime_label: String,
    runtime_tone: &'static str,
    status_summary: String,
}

/// Load the plugins page view-model, optionally selecting a plugin by name.
pub fn load_plugins_page(db: Arc<Database>, selected: Option<String>) -> Result<PluginsPageView> {
    load_plugins_page_filtered(db, selected, PluginStatusFilter::All)
}

/// Load the plugins page view-model with a status filter.
pub fn load_plugins_page_filtered(
    db: Arc<Database>,
    selected: Option<String>,
    filter: PluginStatusFilter,
) -> Result<PluginsPageView> {
    build_page(db, selected, filter, None, String::new())
}

/// Install a plugin from a filesystem path and return the refreshed page.
pub fn install_plugin_from_path(
    db: Arc<Database>,
    input: PluginInstallInput,
) -> Result<PluginsPageView> {
    let source_path = normalize_source_path(input.source_path);

    if source_path.is_empty() {
        return build_page(
            db,
            None,
            PluginStatusFilter::All,
            Some(Notice {
                text: "Plugin path is required.".into(),
                tone: "danger",
            }),
            String::new(),
        );
    }

    if source_path.len() > MAX_PLUGIN_PATH_BYTES {
        return build_page(
            db,
            None,
            PluginStatusFilter::All,
            Some(Notice {
                text: format!(
                    "Plugin path must be {} bytes or less.",
                    MAX_PLUGIN_PATH_BYTES
                ),
                tone: "danger",
            }),
            source_path,
        );
    }

    match install_plugin(db.clone(), source_path.clone().into()) {
        Ok(outcome) => build_page(
            db,
            Some(outcome.plugin.name.clone()),
            PluginStatusFilter::All,
            Some(Notice {
                text: if outcome.registered_skills.is_empty() {
                    format!("Installed plugin `{}`.", outcome.plugin.name)
                } else {
                    format!(
                        "Installed plugin `{}` and registered {} skill(s).",
                        outcome.plugin.name,
                        outcome.registered_skills.len()
                    )
                },
                tone: "success",
            }),
            String::new(),
        ),
        Err(error) => build_page(
            db,
            None,
            PluginStatusFilter::All,
            Some(Notice {
                text: error.to_string(),
                tone: "danger",
            }),
            source_path,
        ),
    }
}

/// Toggle the persisted enabled state for a plugin.
pub fn toggle_plugin_state(db: Arc<Database>, name: String) -> Result<PluginsPageView> {
    let store = PluginStore::new(db.clone());
    let Some(plugin) = store.get_by_name(&name)? else {
        return build_page(
            db,
            None,
            PluginStatusFilter::All,
            Some(Notice {
                text: format!("Plugin `{name}` was not found."),
                tone: "danger",
            }),
            String::new(),
        );
    };

    let enabled = !plugin.enabled;
    if set_plugin_enabled(db.clone(), &name, enabled)? {
        build_page(
            db,
            Some(name),
            PluginStatusFilter::All,
            Some(Notice {
                text: if enabled {
                    "Plugin enabled.".into()
                } else {
                    "Plugin disabled.".into()
                },
                tone: "success",
            }),
            String::new(),
        )
    } else {
        build_page(
            db,
            None,
            PluginStatusFilter::All,
            Some(Notice {
                text: format!("Plugin `{}` was not found.", plugin.name),
                tone: "danger",
            }),
            String::new(),
        )
    }
}

/// Remove a plugin after explicit confirmation.
pub fn delete_plugin(db: Arc<Database>, name: String, confirmed: bool) -> Result<PluginsPageView> {
    if !confirmed {
        return build_page(
            db,
            Some(name.clone()),
            PluginStatusFilter::All,
            Some(Notice {
                text: "Check the confirmation box before removing a plugin.".into(),
                tone: "danger",
            }),
            String::new(),
        );
    }

    let outcome = remove_plugin(db.clone(), &name)?;
    build_page(
        db,
        None,
        PluginStatusFilter::All,
        Some(Notice {
            text: if outcome.removed {
                if outcome.removed_skills.is_empty() {
                    format!("Removed plugin `{name}`.")
                } else {
                    format!(
                        "Removed plugin `{name}` and cleaned up {} skill(s).",
                        outcome.removed_skills.len()
                    )
                }
            } else {
                format!("Plugin `{name}` was already removed.")
            },
            tone: if outcome.removed { "success" } else { "danger" },
        }),
        String::new(),
    )
}

fn build_page(
    db: Arc<Database>,
    selected: Option<String>,
    filter: PluginStatusFilter,
    notice: Option<Notice>,
    install_source_path: String,
) -> Result<PluginsPageView> {
    let skill_store = SkillStore::new().ok();
    build_page_with_skill_store(
        db,
        selected,
        filter,
        notice,
        install_source_path,
        skill_store.as_ref(),
    )
}

fn build_page_with_skill_store(
    db: Arc<Database>,
    selected: Option<String>,
    filter: PluginStatusFilter,
    notice: Option<Notice>,
    install_source_path: String,
    skill_store: Option<&SkillStore>,
) -> Result<PluginsPageView> {
    let states = PluginStore::new(db)
        .list()?
        .into_iter()
        .map(|plugin| build_plugin_state(plugin, skill_store))
        .collect::<Vec<_>>();
    let counts = collect_status_counts(&states);
    let selected_filter = filter;
    let filtered_states = states
        .iter()
        .filter(|state| selected_filter.matches(state.bucket))
        .collect::<Vec<_>>();
    let selected_name = selected
        .filter(|target| {
            filtered_states
                .iter()
                .any(|state| state.plugin.name == *target)
        })
        .or_else(|| {
            filtered_states
                .first()
                .map(|state| state.plugin.name.clone())
        });
    let total_plugins = states.len();

    Ok(PluginsPageView {
        mode_label: if total_plugins == 0 {
            "No plugins installed".into()
        } else {
            format!(
                "{} operational · {} attention · {} disabled",
                counts.operational, counts.attention, counts.disabled
            )
        },
        mode_tone: if total_plugins == 0 {
            "neutral"
        } else if counts.attention > 0 {
            "amber"
        } else {
            "success"
        },
        filters: build_filter_items(counts, selected_filter),
        plugins: filtered_states
            .iter()
            .map(|state| build_plugin_list_item(state, selected_name.as_deref(), selected_filter))
            .collect(),
        selected: match selected_name.as_deref().and_then(|name| {
            filtered_states
                .iter()
                .find(|state| state.plugin.name == name)
        }) {
            Some(state) => build_plugin_detail(state, notice, install_source_path),
            None => placeholder_plugin_detail(
                notice,
                install_source_path,
                selected_filter,
                total_plugins,
            ),
        },
    })
}

fn build_filter_items(
    counts: PluginStatusCounts,
    active_filter: PluginStatusFilter,
) -> Vec<PluginFilterItem> {
    [
        (
            PluginStatusFilter::All,
            counts.operational + counts.attention + counts.disabled,
        ),
        (PluginStatusFilter::Operational, counts.operational),
        (PluginStatusFilter::Attention, counts.attention),
        (PluginStatusFilter::Disabled, counts.disabled),
    ]
    .into_iter()
    .map(|(filter, count)| PluginFilterItem {
        label: filter.label().into(),
        count,
        tone: filter.tone(),
        page_url: plugins_page_url(None, filter),
        active: filter == active_filter,
    })
    .collect()
}

fn build_plugin_state(plugin: Plugin, skill_store: Option<&SkillStore>) -> PluginState {
    let snapshot = plugin_status_snapshot(&plugin, skill_store);
    let requires_runtime = snapshot
        .capabilities
        .iter()
        .any(|capability| capability == "skill" || capability == "channel_adapter");
    let lifecycle_label = if plugin.enabled {
        "Enabled"
    } else {
        "Disabled"
    }
    .to_string();
    let lifecycle_tone = if plugin.enabled { "sage" } else { "neutral" };
    let runtime_note = snapshot.runtime_note.clone().unwrap_or_default();

    let (bucket, status_label, status_tone, runtime_label, runtime_tone, status_summary) =
        if !plugin.enabled {
            (
                PluginStatusBucket::Disabled,
                "Disabled".to_string(),
                "neutral",
                "Runtime paused".to_string(),
                "neutral",
                "Runtime checks pause while the plugin is disabled.".to_string(),
            )
        } else if snapshot.runtime_initialized {
            (
                PluginStatusBucket::Operational,
                "Ready".to_string(),
                "success",
                "Runtime initialized".to_string(),
                "success",
                if snapshot.registered_skills.is_empty() {
                    "Declared runtime capabilities are available.".to_string()
                } else {
                    format!(
                        "{} declared skill(s) are registered in the active runtime.",
                        snapshot.registered_skills.len()
                    )
                },
            )
        } else if !requires_runtime {
            (
                PluginStatusBucket::Operational,
                "Installed".to_string(),
                "cyan",
                "No runtime required".to_string(),
                "cyan",
                "This plugin does not declare a live runtime capability.".to_string(),
            )
        } else if !snapshot.missing_skills.is_empty() {
            (
                PluginStatusBucket::Attention,
                "Missing skills".to_string(),
                "danger",
                format!("{} skill(s) missing", snapshot.missing_skills.len()),
                "danger",
                format!(
                    "{} declared skill(s) are missing from the active runtime.",
                    snapshot.missing_skills.len()
                ),
            )
        } else if runtime_note.contains("manifest unavailable") {
            (
                PluginStatusBucket::Attention,
                "Manifest missing".to_string(),
                "danger",
                "Manifest unavailable".to_string(),
                "danger",
                "The plugin manifest could not be loaded from disk.".to_string(),
            )
        } else if runtime_note.contains("channel adapter runtime loading is not implemented yet") {
            (
                PluginStatusBucket::Attention,
                "Adapter pending".to_string(),
                "amber",
                "Channel adapter pending".to_string(),
                "amber",
                "Channel adapter loading is not wired into the runtime yet.".to_string(),
            )
        } else if runtime_note.contains("skill store unavailable") {
            (
                PluginStatusBucket::Attention,
                "Runtime unknown".to_string(),
                "amber",
                "Skill store unavailable".to_string(),
                "amber",
                "The active skill store could not be loaded for verification.".to_string(),
            )
        } else {
            (
                PluginStatusBucket::Attention,
                "Needs attention".to_string(),
                "amber",
                "Runtime pending".to_string(),
                "amber",
                if runtime_note.is_empty() {
                    "Runtime initialization still needs operator attention.".to_string()
                } else {
                    runtime_note.clone()
                },
            )
        };

    PluginState {
        plugin,
        snapshot,
        bucket,
        status_label,
        status_tone,
        lifecycle_label,
        lifecycle_tone,
        runtime_label,
        runtime_tone,
        status_summary,
    }
}

fn build_plugin_list_item(
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

fn build_plugin_detail(
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

fn placeholder_plugin_detail(
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

fn collect_status_counts(states: &[PluginState]) -> PluginStatusCounts {
    states
        .iter()
        .fold(PluginStatusCounts::default(), |mut counts, state| {
            match state.bucket {
                PluginStatusBucket::Operational => counts.operational += 1,
                PluginStatusBucket::Attention => counts.attention += 1,
                PluginStatusBucket::Disabled => counts.disabled += 1,
            }
            counts
        })
}

fn build_plugin_search_text(state: &PluginState) -> String {
    let capabilities = state.snapshot.capabilities.join(" ");
    let registered_skills = state.snapshot.registered_skills.join(" ");
    let missing_skills = state.snapshot.missing_skills.join(" ");

    [
        state.status_summary.as_str(),
        state.snapshot.runtime_note.as_deref().unwrap_or_default(),
        capabilities.as_str(),
        registered_skills.as_str(),
        missing_skills.as_str(),
    ]
    .into_iter()
    .filter(|segment| !segment.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

fn plugins_page_url(selected: Option<&str>, filter: PluginStatusFilter) -> String {
    let mut query = Vec::new();
    if let Some(filter_value) = filter.query_value() {
        query.push(format!("status={}", encode(filter_value)));
    }
    if let Some(selected) = selected {
        query.push(format!("plugin={}", encode(selected)));
    }

    if query.is_empty() {
        "/plugins".into()
    } else {
        format!("/plugins?{}", query.join("&"))
    }
}

fn normalize_source_path(value: String) -> String {
    value.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn load_plugins_page_empty_returns_placeholder() {
        let page = load_plugins_page(test_db(), None).unwrap();
        assert!(page.plugins.is_empty());
        assert!(page.selected.is_placeholder);
        assert_eq!(page.mode_tone, "neutral");
        assert_eq!(page.filters.len(), 4);
    }

    #[test]
    fn load_plugins_page_selects_first_plugin_by_default() {
        let db = test_db();
        let store = PluginStore::new(db.clone());
        store
            .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
            .unwrap();
        store
            .install("beta", "2.0.0", "/tmp/b", Some("Dev"), None, "")
            .unwrap();

        let page = load_plugins_page(db, None).unwrap();
        assert_eq!(page.plugins.len(), 2);
        assert_eq!(page.selected.title, "alpha");
        assert!(page.plugins[0].active);
    }

    #[test]
    fn load_plugins_page_named_selection_marks_active_item() {
        let db = test_db();
        let store = PluginStore::new(db.clone());
        store
            .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
            .unwrap();
        store
            .install(
                "beta",
                "2.0.0",
                "/tmp/b",
                Some("Dev"),
                None,
                "skill,channel_adapter",
            )
            .unwrap();

        let page = load_plugins_page(db, Some("beta".into())).unwrap();
        assert_eq!(page.selected.title, "beta");
        assert!(
            page.plugins
                .iter()
                .find(|plugin| plugin.title == "beta")
                .unwrap()
                .active
        );
    }

    #[test]
    fn toggle_plugin_state_updates_notice_and_status() {
        let db = test_db();
        let store = PluginStore::new(db.clone());
        store
            .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
            .unwrap();

        let page = toggle_plugin_state(db.clone(), "alpha".into()).unwrap();
        assert_eq!(page.selected.lifecycle_label, "Disabled");

        let toggled = PluginStore::new(db).get_by_name("alpha").unwrap().unwrap();
        assert!(!toggled.enabled);
    }

    #[test]
    fn delete_plugin_requires_confirmation() {
        let db = test_db();
        PluginStore::new(db.clone())
            .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
            .unwrap();

        let page = delete_plugin(db, "alpha".into(), false).unwrap();
        assert_eq!(page.selected.title, "alpha");
        assert_eq!(page.selected.notice.unwrap().tone, "danger");
    }

    #[test]
    fn filtered_page_uses_runtime_snapshot_state() {
        let db = test_db();
        let store = PluginStore::new(db.clone());
        let temp = tempfile::tempdir().expect("temp dir");
        let skill_store = SkillStore::with_dir(temp.path().join("skills"));
        let ready_dir = temp.path().join("ready-tools");
        let missing_dir = temp.path().join("missing-tools");

        std::fs::create_dir_all(&ready_dir).expect("ready plugin dir");
        std::fs::write(
            ready_dir.join("plugin.toml"),
            r#"
name = "ready-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "ls"
cmd = "ls"
"#,
        )
        .expect("ready manifest");
        std::fs::create_dir_all(&missing_dir).expect("missing plugin dir");
        std::fs::write(
            missing_dir.join("plugin.toml"),
            r#"
name = "missing-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "grep"
cmd = "grep"
"#,
        )
        .expect("missing manifest");

        let ready_manifest =
            opengoose_teams::plugin::load_manifest(&ready_dir.join("plugin.toml")).unwrap();
        let ready_loaded =
            opengoose_teams::plugin::LoadedPlugin::from_manifest(ready_manifest, ready_dir.clone());
        opengoose_teams::plugin::PluginRuntime::init_plugin(&ready_loaded, &skill_store).unwrap();

        store
            .install(
                "ready-tools",
                "1.0.0",
                &ready_dir.to_string_lossy(),
                None,
                Some("Ready plugin"),
                "skill",
            )
            .unwrap();
        store
            .install(
                "missing-tools",
                "1.0.0",
                &missing_dir.to_string_lossy(),
                None,
                Some("Needs registration"),
                "skill",
            )
            .unwrap();
        store
            .install("disabled-tools", "1.0.0", "/tmp/disabled", None, None, "")
            .unwrap();
        store.set_enabled("disabled-tools", false).unwrap();

        let page = build_page_with_skill_store(
            db,
            None,
            PluginStatusFilter::Attention,
            None,
            String::new(),
            Some(&skill_store),
        )
        .unwrap();

        assert_eq!(page.plugins.len(), 1);
        assert_eq!(page.plugins[0].title, "missing-tools");
        assert_eq!(page.plugins[0].status_label, "Missing skills");
        assert!(page.plugins[0].page_url.contains("status=attention"));
        assert_eq!(page.selected.runtime_label, "1 skill(s) missing");
        assert_eq!(page.mode_label, "1 operational · 1 attention · 1 disabled");
    }

    #[test]
    fn filtered_page_shows_placeholder_when_nothing_matches() {
        let db = test_db();
        PluginStore::new(db.clone())
            .install("alpha", "1.0.0", "/tmp/a", None, None, "")
            .unwrap();

        let page = build_page_with_skill_store(
            db,
            None,
            PluginStatusFilter::Disabled,
            None,
            String::new(),
            None,
        )
        .unwrap();

        assert!(page.plugins.is_empty());
        assert!(page.selected.is_placeholder);
        assert_eq!(page.selected.title, "No disabled plugins");
    }
}
