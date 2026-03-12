use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};

use opengoose_persistence::{Database, Plugin, PluginStore};
use opengoose_profiles::SkillStore;
use opengoose_teams::plugin::{LoadedPlugin, PluginRuntime, load_manifest};

pub struct PluginInstallOutcome {
    pub plugin: Plugin,
    pub registered_skills: Vec<String>,
}

pub struct PluginRemoveOutcome {
    pub removed: bool,
    pub removed_skills: Vec<String>,
}

pub fn install_plugin(db: Arc<Database>, path: PathBuf) -> Result<PluginInstallOutcome> {
    let path = path.canonicalize().map_err(|_| {
        anyhow!(
            "plugin path '{}' does not exist or is not accessible",
            path.display()
        )
    })?;

    if !path.is_dir() {
        bail!(
            "'{}' is not a directory. A plugin must be a directory containing plugin.toml.",
            path.display()
        );
    }

    let manifest_path = path.join("plugin.toml");
    let manifest = load_manifest(&manifest_path).with_context(|| {
        format!(
            "failed to load plugin manifest at {}",
            manifest_path.display()
        )
    })?;

    let store = PluginStore::new(db);
    if store.get_by_name(&manifest.name)?.is_some() {
        bail!(
            "plugin '{}' is already installed. Remove it first with `opengoose plugin remove {}`.",
            manifest.name,
            manifest.name
        );
    }

    let loaded = LoadedPlugin::from_manifest(manifest.clone(), path.clone());
    let registered_skills = match SkillStore::new() {
        Ok(skill_store) => {
            PluginRuntime::init_plugin(&loaded, &skill_store)
                .with_context(|| format!("failed to initialize plugin '{}'", manifest.name))?
                .registered_skills
        }
        Err(_) => Vec::new(),
    };

    let plugin = store.install(
        &manifest.name,
        &manifest.version,
        &path.to_string_lossy(),
        manifest.author.as_deref(),
        manifest.description.as_deref(),
        &manifest.capabilities_str(),
    )?;

    Ok(PluginInstallOutcome {
        plugin,
        registered_skills,
    })
}

pub fn remove_plugin(db: Arc<Database>, name: &str) -> Result<PluginRemoveOutcome> {
    let store = PluginStore::new(db);
    let removed_skills = store
        .get_by_name(name)?
        .map(|plugin| shutdown_plugin_skills(&plugin))
        .transpose()?
        .unwrap_or_default();

    let removed = store.uninstall(name)?;
    Ok(PluginRemoveOutcome {
        removed,
        removed_skills,
    })
}

pub fn set_plugin_enabled(db: Arc<Database>, name: &str, enabled: bool) -> Result<bool> {
    PluginStore::new(db)
        .set_enabled(name, enabled)
        .map_err(Into::into)
}

fn shutdown_plugin_skills(plugin: &Plugin) -> Result<Vec<String>> {
    let source = Path::new(&plugin.source_path);
    let manifest_path = source.join("plugin.toml");
    if !manifest_path.exists() {
        return Ok(Vec::new());
    }

    let manifest = match load_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(_) => return Ok(Vec::new()),
    };
    let skill_store = match SkillStore::new() {
        Ok(skill_store) => skill_store,
        Err(_) => return Ok(Vec::new()),
    };

    PluginRuntime::shutdown_plugin(
        &LoadedPlugin::from_manifest(manifest, source.to_path_buf()),
        &skill_store,
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use tempfile::TempDir;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().expect("db should open"))
    }

    fn plugin_dir(name: &str, version: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("temp dir should build");
        let path = dir.path().join(name);
        fs::create_dir_all(&path).expect("plugin dir should exist");
        fs::write(
            path.join("plugin.toml"),
            format!(
                "name = \"{name}\"\nversion = \"{version}\"\ndescription = \"Test plugin\"\ncapabilities = [\"skill\"]\n\n[[skills]]\nname = \"echo\"\ncmd = \"echo\"\nargs = [\"hello\"]\ndescription = \"Echo text\"\n"
            ),
        )
        .expect("manifest should write");
        (dir, path)
    }

    #[test]
    fn install_plugin_persists_manifest_metadata() {
        let db = test_db();
        let (_dir, path) = plugin_dir("sample-plugin", "1.2.3");

        let outcome = install_plugin(db.clone(), path).expect("plugin should install");

        assert_eq!(outcome.plugin.name, "sample-plugin");
        assert_eq!(outcome.plugin.version, "1.2.3");
        assert!(
            PluginStore::new(db)
                .get_by_name("sample-plugin")
                .expect("lookup should succeed")
                .is_some()
        );
    }

    #[test]
    fn remove_plugin_reports_removed_flag() {
        let db = test_db();
        let (_dir, path) = plugin_dir("removable-plugin", "0.1.0");
        install_plugin(db.clone(), path).expect("plugin should install");

        let outcome = remove_plugin(db.clone(), "removable-plugin").expect("remove should work");

        assert!(outcome.removed);
        assert!(
            PluginStore::new(db)
                .get_by_name("removable-plugin")
                .expect("lookup should succeed")
                .is_none()
        );
    }

    #[test]
    fn set_plugin_enabled_updates_state() {
        let db = test_db();
        let (_dir, path) = plugin_dir("toggle-plugin", "0.1.0");
        install_plugin(db.clone(), path).expect("plugin should install");

        assert!(set_plugin_enabled(db.clone(), "toggle-plugin", false).expect("disable works"));
        assert!(
            !PluginStore::new(db.clone())
                .get_by_name("toggle-plugin")
                .expect("lookup should succeed")
                .expect("plugin should exist")
                .enabled
        );

        assert!(set_plugin_enabled(db.clone(), "toggle-plugin", true).expect("enable works"));
        assert!(
            PluginStore::new(db)
                .get_by_name("toggle-plugin")
                .expect("lookup should succeed")
                .expect("plugin should exist")
                .enabled
        );
    }
}
