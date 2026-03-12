use std::sync::Arc;

use diesel::prelude::*;
use tracing::debug;

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::{NewPlugin, PluginRow};
use crate::schema::plugins;

/// A registered plugin with its metadata.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub id: i32,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    /// Comma-separated capability tags (e.g. "skill,channel_adapter")
    pub capabilities: String,
    /// Path to the plugin file or directory on disk.
    pub source_path: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Plugin {
    fn from_row(row: PluginRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            version: row.version,
            author: row.author,
            description: row.description,
            capabilities: row.capabilities,
            source_path: row.source_path,
            enabled: row.enabled != 0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }

    /// Split `capabilities` string into individual tags.
    pub fn capability_list(&self) -> Vec<&str> {
        self.capabilities
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// CRUD operations on the `plugins` table.
pub struct PluginStore {
    db: Arc<Database>,
}

impl PluginStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Install (register) a plugin.
    pub fn install(
        &self,
        name: &str,
        version: &str,
        source_path: &str,
        author: Option<&str>,
        description: Option<&str>,
        capabilities: &str,
    ) -> PersistenceResult<Plugin> {
        self.db.with(|conn| {
            let row = diesel::insert_into(plugins::table)
                .values(NewPlugin {
                    name,
                    version,
                    source_path,
                    author,
                    description,
                    capabilities,
                })
                .get_result::<PluginRow>(conn)?;

            debug!(name, version, source_path, "plugin installed");
            Ok(Plugin::from_row(row))
        })
    }

    /// List all plugins.
    pub fn list(&self) -> PersistenceResult<Vec<Plugin>> {
        self.db.with(|conn| {
            let rows = plugins::table
                .order(plugins::name.asc())
                .load::<PluginRow>(conn)?;
            Ok(rows.into_iter().map(Plugin::from_row).collect())
        })
    }

    /// Get a plugin by name.
    pub fn get_by_name(&self, name: &str) -> PersistenceResult<Option<Plugin>> {
        self.db.with(|conn| {
            let result = plugins::table
                .filter(plugins::name.eq(name))
                .first::<PluginRow>(conn)
                .optional()?;
            Ok(result.map(Plugin::from_row))
        })
    }

    /// Uninstall (remove) a plugin by name.
    pub fn uninstall(&self, name: &str) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count =
                diesel::delete(plugins::table.filter(plugins::name.eq(name))).execute(conn)?;
            if count > 0 {
                debug!(name, "plugin uninstalled");
            }
            Ok(count > 0)
        })
    }

    /// Enable or disable a plugin.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> PersistenceResult<bool> {
        self.db.with(|conn| {
            let count = diesel::update(plugins::table.filter(plugins::name.eq(name)))
                .set((
                    plugins::enabled.eq(if enabled { 1 } else { 0 }),
                    plugins::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(count > 0)
        })
    }

    /// List only enabled plugins.
    pub fn list_enabled(&self) -> PersistenceResult<Vec<Plugin>> {
        self.db.with(|conn| {
            let rows = plugins::table
                .filter(plugins::enabled.eq(1))
                .order(plugins::name.asc())
                .load::<PluginRow>(conn)?;
            Ok(rows.into_iter().map(Plugin::from_row).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::test_db;

    #[test]
    fn test_install_and_get() {
        let db = test_db();
        let store = PluginStore::new(db);

        let plugin = store
            .install(
                "my-plugin",
                "1.0.0",
                "/home/user/.opengoose/plugins/my-plugin",
                Some("Alice"),
                Some("A test plugin"),
                "skill",
            )
            .unwrap();

        assert_eq!(plugin.name, "my-plugin");
        assert_eq!(plugin.version, "1.0.0");
        assert!(plugin.enabled);
        assert_eq!(plugin.author.as_deref(), Some("Alice"));

        let fetched = store.get_by_name("my-plugin").unwrap().unwrap();
        assert_eq!(
            fetched.source_path,
            "/home/user/.opengoose/plugins/my-plugin"
        );
    }

    #[test]
    fn test_list() {
        let db = test_db();
        let store = PluginStore::new(db);

        store
            .install("alpha", "1.0", "/a", None, None, "skill")
            .unwrap();
        store
            .install("beta", "2.0", "/b", None, None, "channel_adapter")
            .unwrap();

        let all = store.list().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "alpha");
        assert_eq!(all[1].name, "beta");
    }

    #[test]
    fn test_uninstall() {
        let db = test_db();
        let store = PluginStore::new(db);

        store
            .install("temp", "0.1", "/tmp/temp", None, None, "")
            .unwrap();
        assert!(store.uninstall("temp").unwrap());
        assert!(!store.uninstall("temp").unwrap());
        assert!(store.get_by_name("temp").unwrap().is_none());
    }

    #[test]
    fn test_enable_disable() {
        let db = test_db();
        let store = PluginStore::new(db);

        store
            .install("plug", "1.0", "/p", None, None, "skill")
            .unwrap();

        store.set_enabled("plug", false).unwrap();
        let p = store.get_by_name("plug").unwrap().unwrap();
        assert!(!p.enabled);

        store.set_enabled("plug", true).unwrap();
        let p = store.get_by_name("plug").unwrap().unwrap();
        assert!(p.enabled);
    }

    #[test]
    fn test_list_enabled() {
        let db = test_db();
        let store = PluginStore::new(db);

        store
            .install("p1", "1.0", "/p1", None, None, "skill")
            .unwrap();
        store
            .install("p2", "1.0", "/p2", None, None, "skill")
            .unwrap();
        store.set_enabled("p2", false).unwrap();

        let enabled = store.list_enabled().unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "p1");
    }

    #[test]
    fn test_capability_list() {
        let db = test_db();
        let store = PluginStore::new(db);

        store
            .install("multi", "1.0", "/m", None, None, "skill, channel_adapter")
            .unwrap();

        let p = store.get_by_name("multi").unwrap().unwrap();
        let caps = p.capability_list();
        assert_eq!(caps, vec!["skill", "channel_adapter"]);
    }

    #[test]
    fn test_get_nonexistent() {
        let db = test_db();
        let store = PluginStore::new(db);
        assert!(store.get_by_name("no-such").unwrap().is_none());
    }
}
