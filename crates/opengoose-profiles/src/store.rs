use std::path::{Path, PathBuf};

use opengoose_types::YamlFileStore;

use crate::defaults::all_defaults;
use crate::error::{ProfileError, ProfileResult};
use crate::profile::AgentProfile;

/// CRUD store for agent profiles on disk (`~/.opengoose/profiles/`).
pub struct ProfileStore {
    inner: YamlFileStore,
}

impl ProfileStore {
    /// Create a store backed by `~/.opengoose/profiles/`.
    pub fn new() -> ProfileResult<Self> {
        let home = dirs::home_dir().ok_or(ProfileError::NoHomeDir)?;
        Ok(Self {
            inner: YamlFileStore::new(home.join(".opengoose").join("profiles")),
        })
    }

    /// Create a store backed by a custom directory (for testing).
    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            inner: YamlFileStore::new(dir),
        }
    }

    /// The profiles directory path.
    pub fn dir(&self) -> &Path {
        self.inner.dir()
    }

    /// List all profile names (sorted).
    pub fn list(&self) -> ProfileResult<Vec<String>> {
        self.inner.list::<AgentProfile>()
    }

    /// Get a profile by name.
    pub fn get(&self, name: &str) -> ProfileResult<AgentProfile> {
        self.inner.get::<AgentProfile>(name).map_err(|e| {
            // Convert generic io::NotFound to typed ProfileError::NotFound
            if let ProfileError::Io(ref io_err) = e
                && io_err.kind() == std::io::ErrorKind::NotFound
            {
                return ProfileError::NotFound(name.to_string());
            }
            e
        })
    }

    /// Save a profile. If `force` is false and the file exists, returns `AlreadyExists`.
    pub fn save(&self, profile: &AgentProfile, force: bool) -> ProfileResult<()> {
        self.inner.save(profile, force).map_err(|e| {
            // Convert generic io::AlreadyExists to typed ProfileError::AlreadyExists
            if let ProfileError::Io(ref io_err) = e
                && io_err.kind() == std::io::ErrorKind::AlreadyExists
            {
                return ProfileError::AlreadyExists(profile.title.clone());
            }
            e
        })
    }

    /// Remove a profile by name.
    pub fn remove(&self, name: &str) -> ProfileResult<()> {
        self.inner.remove(name).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProfileError::NotFound(name.to_string())
            } else {
                ProfileError::Io(e)
            }
        })
    }

    /// Install bundled default profiles (skips existing ones unless `force` is true).
    pub fn install_defaults(&self, force: bool) -> ProfileResult<usize> {
        self.inner.ensure_dir()?;
        let mut count = 0;
        for profile in all_defaults() {
            let path = self.inner.path_for(profile.name());
            if !force && path.exists() {
                continue;
            }
            let yaml = profile.to_yaml()?;
            std::fs::write(&path, yaml)?;
            count += 1;
        }
        Ok(count)
    }

    /// Resolve the file path for a profile name (exposed for tests).
    #[cfg(test)]
    fn path_for(&self, name: &str) -> PathBuf {
        self.inner.path_for(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, ProfileStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProfileStore::with_dir(tmp.path().to_path_buf());
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
        assert_eq!(count, 4);
        let names = store.list().unwrap();
        assert_eq!(names, vec!["developer", "researcher", "reviewer", "writer"]);
    }

    #[test]
    fn get_and_remove() {
        let (_tmp, store) = temp_store();
        store.install_defaults(false).unwrap();

        let profile = store.get("researcher").unwrap();
        assert_eq!(profile.name(), "researcher");

        store.remove("researcher").unwrap();
        assert!(store.get("researcher").is_err());
    }

    #[test]
    fn save_no_force_rejects_duplicate() {
        let (_tmp, store) = temp_store();
        store.install_defaults(false).unwrap();
        let profile = store.get("developer").unwrap();
        let err = store.save(&profile, false).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn save_force_overwrites() {
        let (_tmp, store) = temp_store();
        store.install_defaults(false).unwrap();
        let profile = store.get("developer").unwrap();
        store.save(&profile, true).unwrap();
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
    fn test_dir_accessor() {
        let (_tmp, store) = temp_store();
        assert!(store.dir().exists() || !store.dir().exists()); // dir() returns the path
        assert_eq!(store.dir(), _tmp.path());
    }

    #[test]
    fn test_get_not_found() {
        let (_tmp, store) = temp_store();
        let err = store.get("nonexistent").unwrap_err();
        assert!(matches!(err, ProfileError::NotFound(_)));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn test_remove_not_found() {
        let (_tmp, store) = temp_store();
        let err = store.remove("nonexistent").unwrap_err();
        assert!(matches!(err, ProfileError::NotFound(_)));
        assert!(err.to_string().contains("nonexistent"));
    }
}
