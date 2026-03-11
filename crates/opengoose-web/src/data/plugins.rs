use std::sync::Arc;

use anyhow::Result;
use opengoose_core::plugins::{install_plugin, remove_plugin, set_plugin_enabled};
use opengoose_persistence::{Database, Plugin, PluginStore};
use urlencoding::encode;

use crate::data::utils::{preview, source_badge};
use crate::data::views::{MetaRow, Notice, PluginDetailView, PluginListItem, PluginsPageView};

const MAX_PLUGIN_PATH_BYTES: usize = 4096;

pub struct PluginInstallInput {
    pub source_path: String,
}

/// Load the plugins page view-model, optionally selecting a plugin by name.
pub fn load_plugins_page(db: Arc<Database>, selected: Option<String>) -> Result<PluginsPageView> {
    build_page(db, selected, None, String::new())
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
    notice: Option<Notice>,
    install_source_path: String,
) -> Result<PluginsPageView> {
    let plugins = PluginStore::new(db).list()?;
    let selected_name = selected
        .filter(|target| plugins.iter().any(|plugin| plugin.name == *target))
        .or_else(|| plugins.first().map(|plugin| plugin.name.clone()));

    Ok(PluginsPageView {
        mode_label: if plugins.is_empty() {
            "No plugins installed".into()
        } else {
            format!("{} plugin(s) installed", plugins.len())
        },
        mode_tone: if plugins.is_empty() {
            "neutral"
        } else {
            "success"
        },
        plugins: plugins
            .iter()
            .map(|plugin| build_plugin_list_item(plugin, selected_name.as_deref()))
            .collect(),
        selected: match selected_name
            .as_deref()
            .and_then(|name| plugins.iter().find(|plugin| plugin.name == name))
        {
            Some(plugin) => build_plugin_detail(plugin, notice, install_source_path),
            None => placeholder_plugin_detail(notice, install_source_path),
        },
    })
}

fn build_plugin_list_item(plugin: &Plugin, selected_name: Option<&str>) -> PluginListItem {
    PluginListItem {
        title: plugin.name.clone(),
        subtitle: match plugin.author.as_deref() {
            Some(author) if !author.trim().is_empty() => {
                format!("v{} · {}", plugin.version, author)
            }
            _ => format!("v{}", plugin.version),
        },
        preview: plugin
            .description
            .as_deref()
            .map(|description| preview(description, 84))
            .unwrap_or_else(|| "No plugin description provided.".into()),
        source_label: plugin.source_path.clone(),
        source_badge: source_badge(&plugin.source_path),
        status_label: if plugin.enabled {
            "Enabled".into()
        } else {
            "Disabled".into()
        },
        status_tone: if plugin.enabled { "sage" } else { "neutral" },
        page_url: format!("/plugins?plugin={}", encode(&plugin.name)),
        active: selected_name == Some(plugin.name.as_str()),
    }
}

fn build_plugin_detail(
    plugin: &Plugin,
    notice: Option<Notice>,
    install_source_path: String,
) -> PluginDetailView {
    let capabilities = plugin
        .capability_list()
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    PluginDetailView {
        title: plugin.name.clone(),
        subtitle: plugin.description.clone().unwrap_or_else(|| {
            "This plugin is installed locally and ready for operator review.".into()
        }),
        source_label: plugin.source_path.clone(),
        status_label: if plugin.enabled {
            "Enabled".into()
        } else {
            "Disabled".into()
        },
        status_tone: if plugin.enabled { "sage" } else { "neutral" },
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
                label: "Installed".into(),
                value: plugin.created_at.clone(),
            },
            MetaRow {
                label: "Updated".into(),
                value: plugin.updated_at.clone(),
            },
            MetaRow {
                label: "Path".into(),
                value: plugin.source_path.clone(),
            },
        ],
        capabilities_hint: "No capabilities declared in plugin.toml.".into(),
        capabilities,
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
) -> PluginDetailView {
    PluginDetailView {
        title: "No plugins installed".into(),
        subtitle: "Install a plugin directory with a plugin.toml manifest to start managing plugin lifecycle from the dashboard.".into(),
        source_label: "Local plugin registry".into(),
        status_label: "Awaiting install".into(),
        status_tone: "neutral",
        meta: vec![],
        capabilities: vec![],
        capabilities_hint: "Installed plugin capabilities will appear here.".into(),
        notice,
        install_source_path,
        toggle_label: String::new(),
        delete_label: String::new(),
        is_placeholder: true,
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
        assert_eq!(page.selected.capabilities.len(), 2);
    }

    #[test]
    fn toggle_plugin_state_updates_notice_and_status() {
        let db = test_db();
        let store = PluginStore::new(db.clone());
        store
            .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
            .unwrap();

        let page = toggle_plugin_state(db.clone(), "alpha".into()).unwrap();
        assert_eq!(page.selected.status_label, "Disabled");

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
}
