use std::sync::Arc;

use anyhow::Result;
use opengoose_core::plugins::{install_plugin, remove_plugin, set_plugin_enabled};
use opengoose_persistence::{Database, PluginStore};

use super::PluginStatusFilter;
use super::catalog::build_page;
use crate::data::views::{Notice, PluginsPageView};

const MAX_PLUGIN_PATH_BYTES: usize = 4096;

pub struct PluginInstallInput {
    pub source_path: String,
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

fn normalize_source_path(value: String) -> String {
    value.trim().to_string()
}
