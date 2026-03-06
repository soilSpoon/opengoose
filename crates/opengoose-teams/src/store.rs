use std::path::{Path, PathBuf};

use opengoose_types::YamlFileStore;

use crate::defaults::all_defaults;
use crate::error::{TeamError, TeamResult};
use crate::team::TeamDefinition;

/// CRUD store for team definitions on disk (`~/.opengoose/teams/`).
pub struct TeamStore {
    inner: YamlFileStore,
}

impl TeamStore {
    /// Create a store backed by `~/.opengoose/teams/`.
    pub fn new() -> TeamResult<Self> {
        let home = dirs::home_dir().ok_or(TeamError::NoHomeDir)?;
        Ok(Self {
            inner: YamlFileStore::new(home.join(".opengoose").join("teams")),
        })
    }

    /// Create a store backed by a custom directory (for testing).
    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            inner: YamlFileStore::new(dir),
        }
    }

    /// The teams directory path.
    pub fn dir(&self) -> &Path {
        self.inner.dir()
    }

    /// List all team names (sorted).
    pub fn list(&self) -> TeamResult<Vec<String>> {
        self.inner.list::<TeamDefinition>()
    }

    /// Get a team by name.
    pub fn get(&self, name: &str) -> TeamResult<TeamDefinition> {
        self.inner.get::<TeamDefinition>(name).map_err(|e| {
            if let TeamError::Io(ref io_err) = e
                && io_err.kind() == std::io::ErrorKind::NotFound
            {
                return TeamError::NotFound(name.to_string());
            }
            e
        })
    }

    /// Save a team. If `force` is false and the file exists, returns `AlreadyExists`.
    pub fn save(&self, team: &TeamDefinition, force: bool) -> TeamResult<()> {
        self.inner.save(team, force).map_err(|e| {
            if let TeamError::Io(ref io_err) = e
                && io_err.kind() == std::io::ErrorKind::AlreadyExists
            {
                return TeamError::AlreadyExists(team.title.clone());
            }
            e
        })
    }

    /// Remove a team by name.
    pub fn remove(&self, name: &str) -> TeamResult<()> {
        self.inner.remove(name).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                TeamError::NotFound(name.to_string())
            } else {
                TeamError::Io(e)
            }
        })
    }

    /// Install bundled default teams (skips existing ones unless `force` is true).
    pub fn install_defaults(&self, force: bool) -> TeamResult<usize> {
        self.inner.ensure_dir()?;
        let mut count = 0;
        for team in all_defaults() {
            let path = self.inner.path_for(team.name());
            if !force && path.exists() {
                continue;
            }
            let yaml = team.to_yaml()?;
            std::fs::write(&path, yaml)?;
            count += 1;
        }
        Ok(count)
    }

    /// Resolve the file path for a team name (exposed for tests).
    #[cfg(test)]
    fn path_for(&self, name: &str) -> PathBuf {
        self.inner.path_for(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, TeamStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = TeamStore::with_dir(tmp.path().to_path_buf());
        (tmp, store)
    }

    #[test]
    fn list_empty() {
        let (_tmp, store) = temp_store();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn install_defaults_and_list() {
        let (_tmp, store) = temp_store();
        let count = store.install_defaults(false).unwrap();
        assert_eq!(count, 3);
        let names = store.list().unwrap();
        assert_eq!(names, vec!["code-review", "research-panel", "smart-router"]);
    }

    #[test]
    fn get_and_remove() {
        let (_tmp, store) = temp_store();
        store.install_defaults(false).unwrap();

        let team = store.get("code-review").unwrap();
        assert_eq!(team.name(), "code-review");

        store.remove("code-review").unwrap();
        assert!(store.get("code-review").is_err());
    }

    #[test]
    fn save_no_force_rejects_duplicate() {
        let (_tmp, store) = temp_store();
        store.install_defaults(false).unwrap();
        let team = store.get("code-review").unwrap();
        let err = store.save(&team, false).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn path_for_sanitizes_traversal() {
        let (_tmp, store) = temp_store();
        let path = store.path_for("../../etc/passwd");
        assert!(path.starts_with(store.dir()));
        assert!(!path.to_string_lossy().contains(".."));
    }

    #[test]
    fn path_for_sanitizes_slashes() {
        let (_tmp, store) = temp_store();
        let path = store.path_for("foo/bar\\baz");
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(!filename.contains('/'));
        assert!(!filename.contains('\\'));
    }

    #[test]
    fn dir_returns_store_path() {
        let (tmp, store) = temp_store();
        assert_eq!(store.dir(), tmp.path());
    }

    #[test]
    fn get_not_found_returns_not_found_error() {
        let (_tmp, store) = temp_store();
        let err = store.get("nonexistent").unwrap_err();
        assert!(matches!(err, TeamError::NotFound(_)));
    }

    #[test]
    fn remove_not_found_returns_not_found_error() {
        let (_tmp, store) = temp_store();
        let err = store.remove("nonexistent").unwrap_err();
        assert!(matches!(err, TeamError::NotFound(_)));
    }

    #[test]
    fn install_defaults_skips_existing() {
        let (_tmp, store) = temp_store();
        let first = store.install_defaults(false).unwrap();
        assert_eq!(first, 3);
        let second = store.install_defaults(false).unwrap();
        assert_eq!(second, 0);
    }

    #[test]
    fn install_defaults_force_overwrites() {
        let (_tmp, store) = temp_store();
        store.install_defaults(false).unwrap();
        let count = store.install_defaults(true).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn save_force_overwrites_existing() {
        let (_tmp, store) = temp_store();
        store.install_defaults(false).unwrap();
        let team = store.get("code-review").unwrap();
        store.save(&team, true).unwrap();
        let reloaded = store.get("code-review").unwrap();
        assert_eq!(reloaded.name(), "code-review");
    }
}
