use std::path::Path;

use opengoose_profiles::{ExtensionRef, ProfileError, Skill, SkillStore};
use opengoose_types::PluginStatusSnapshot;

use crate::error::{TeamError, TeamResult};

use super::loaded::LoadedPlugin;

/// Plugin lifecycle manager that handles skill registration and cleanup.
///
/// `PluginRuntime` bridges the gap between the plugin manifest (what a plugin
/// declares) and the skill store (where registered skills live). It converts
/// `PluginSkillDef` entries from a manifest into `Skill` objects and persists
/// them via the `SkillStore`.
pub struct PluginRuntime;

impl PluginRuntime {
    /// Initialise a plugin: register its skill definitions with the skill store.
    ///
    /// For plugins with the `skill` capability, each `[[skills]]` entry in the
    /// manifest is converted to a `Skill` and saved to the store. Skill names
    /// are prefixed with the plugin name to avoid collisions (e.g.
    /// `my-plugin/git-log`).
    ///
    /// For plugins with the `channel_adapter` capability, a log message is
    /// emitted. Dynamic channel adapter loading is not yet implemented.
    pub fn init_plugin(
        plugin: &LoadedPlugin,
        skill_store: &SkillStore,
    ) -> TeamResult<PluginInitResult> {
        let manifest = plugin.manifest();
        let mut registered_skills = Vec::new();

        if manifest.has_skill_capability() {
            for skill_def in &manifest.skills {
                let skill_name = format!("{}/{}", manifest.name, skill_def.name);
                let extension = ExtensionRef {
                    name: skill_def.name.clone(),
                    ext_type: "stdio".to_string(),
                    cmd: Some(skill_def.cmd.clone()),
                    args: skill_def.args.clone(),
                    uri: None,
                    timeout: skill_def.timeout,
                    envs: skill_def.envs.clone(),
                    env_keys: vec![],
                    code: None,
                    dependencies: None,
                };

                let skill = Skill {
                    name: skill_name.clone(),
                    description: skill_def.description.clone(),
                    version: manifest.version.clone(),
                    extensions: vec![extension],
                };

                // Overwrite if already registered (plugin update path).
                skill_store.save(&skill, true).map_err(|error| {
                    TeamError::PluginInit(format!(
                        "failed to register skill '{}': {}",
                        skill_name, error
                    ))
                })?;

                tracing::info!(
                    plugin = %manifest.name,
                    skill = %skill_name,
                    cmd = %skill_def.cmd,
                    "registered plugin skill"
                );
                registered_skills.push(skill_name);
            }
        }

        if manifest.has_channel_adapter_capability() {
            tracing::info!(
                plugin = %manifest.name,
                "plugin declares channel_adapter capability (dynamic loading not yet supported)"
            );
        }

        Ok(PluginInitResult {
            plugin_name: manifest.name.clone(),
            registered_skills,
        })
    }

    /// Shut down a plugin: remove its registered skills from the skill store.
    ///
    /// Removes all skills prefixed with `{plugin_name}/` from the store.
    pub fn shutdown_plugin(
        plugin: &LoadedPlugin,
        skill_store: &SkillStore,
    ) -> TeamResult<Vec<String>> {
        let manifest = plugin.manifest();
        let mut removed = Vec::new();

        if manifest.has_skill_capability() {
            for skill_def in &manifest.skills {
                let skill_name = format!("{}/{}", manifest.name, skill_def.name);
                match skill_store.remove(&skill_name) {
                    Ok(()) => {
                        tracing::info!(
                            plugin = %manifest.name,
                            skill = %skill_name,
                            "removed plugin skill"
                        );
                        removed.push(skill_name);
                    }
                    Err(ProfileError::SkillNotFound(_)) => {
                        tracing::debug!(
                            plugin = %manifest.name,
                            skill = %skill_name,
                            "skill already removed or never registered"
                        );
                    }
                    Err(error) => {
                        return Err(TeamError::PluginInit(format!(
                            "failed to remove skill '{}': {}",
                            skill_name, error
                        )));
                    }
                }
            }
        }

        Ok(removed)
    }
}

pub fn list_plugin_status_snapshots(
    store: &opengoose_persistence::PluginStore,
    skill_store: Option<&SkillStore>,
) -> TeamResult<Vec<PluginStatusSnapshot>> {
    Ok(store
        .list()?
        .into_iter()
        .map(|plugin| plugin_status_snapshot(&plugin, skill_store))
        .collect())
}

pub fn plugin_status_snapshot(
    plugin: &opengoose_persistence::Plugin,
    skill_store: Option<&SkillStore>,
) -> PluginStatusSnapshot {
    let manifest_path = Path::new(&plugin.source_path).join("plugin.toml");
    let capabilities = plugin
        .capability_list()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut snapshot = PluginStatusSnapshot {
        name: plugin.name.clone(),
        version: plugin.version.clone(),
        enabled: plugin.enabled,
        source_path: plugin.source_path.clone(),
        capabilities: capabilities.clone(),
        runtime_initialized: false,
        registered_skills: Vec::new(),
        missing_skills: Vec::new(),
        runtime_note: None,
    };

    let Ok(manifest) = super::load_manifest(&manifest_path) else {
        snapshot.runtime_note = Some(format!(
            "plugin manifest unavailable at {}",
            manifest_path.display()
        ));
        return snapshot;
    };

    if !manifest.capabilities.is_empty() {
        snapshot.capabilities = manifest.capabilities.clone();
    }

    let expected_skills = manifest
        .skills
        .iter()
        .map(|skill| format!("{}/{}", manifest.name, skill.name))
        .collect::<Vec<_>>();

    match skill_store {
        Some(skill_store) => {
            for skill_name in expected_skills {
                if skill_store.get(&skill_name).is_ok() {
                    snapshot.registered_skills.push(skill_name);
                } else {
                    snapshot.missing_skills.push(skill_name);
                }
            }
        }
        None => {
            snapshot.missing_skills = expected_skills;
        }
    }

    if manifest.has_skill_capability() {
        snapshot.runtime_initialized = snapshot.missing_skills.is_empty();
        let total_declared_skills =
            snapshot.registered_skills.len() + snapshot.missing_skills.len();
        snapshot.runtime_note = Some(match skill_store {
            Some(_) if snapshot.runtime_initialized => {
                format!("registered {total_declared_skills} declared skill(s)")
            }
            Some(_) => format!(
                "missing {} of {total_declared_skills} declared skill(s)",
                snapshot.missing_skills.len()
            ),
            None => "skill store unavailable; runtime registration could not be verified".into(),
        });
    } else if manifest.has_channel_adapter_capability() {
        snapshot.runtime_note =
            Some("channel adapter runtime loading is not implemented yet".into());
    } else if skill_store.is_none() {
        snapshot.runtime_note =
            Some("skill store unavailable; runtime registration could not be verified".into());
    } else {
        snapshot.runtime_note = Some("plugin does not declare a runtime capability".into());
    }

    snapshot
}

/// Result of initializing a plugin.
#[derive(Debug)]
pub struct PluginInitResult {
    /// Name of the plugin that was initialized.
    pub plugin_name: String,
    /// Names of skills that were registered.
    pub registered_skills: Vec<String>,
}
