use std::path::{Path, PathBuf};

use crate::defaults::all_defaults;
use crate::error::{ProfileError, ProfileResult};
use crate::profile::AgentProfile;

/// CRUD store for agent profiles on disk (`~/.opengoose/profiles/`).
pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    /// Create a store backed by `~/.opengoose/profiles/`.
    pub fn new() -> ProfileResult<Self> {
        let home = dirs::home_dir().ok_or(ProfileError::NoHomeDir)?;
        Ok(Self {
            dir: home.join(".opengoose").join("profiles"),
        })
    }

    /// Create a store backed by a custom directory (for testing).
    pub fn with_dir(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The profiles directory path.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Ensure the profiles directory exists.
    fn ensure_dir(&self) -> ProfileResult<()> {
        std::fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    /// List all profile names (sorted).
    pub fn list(&self) -> ProfileResult<Vec<String>> {
        if !self.dir.exists() {
            return Ok(vec![]);
        }

        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "yaml" || ext == "yml") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(profile) = AgentProfile::from_yaml(&content) {
                        names.push(profile.title);
                    }
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// Get a profile by name.
    pub fn get(&self, name: &str) -> ProfileResult<AgentProfile> {
        let path = self.path_for(name);
        if !path.exists() {
            return Err(ProfileError::NotFound(name.to_string()));
        }
        let content = std::fs::read_to_string(&path)?;
        AgentProfile::from_yaml(&content)
    }

    /// Save a profile. If `force` is false and the file exists, returns `AlreadyExists`.
    pub fn save(&self, profile: &AgentProfile, force: bool) -> ProfileResult<()> {
        self.ensure_dir()?;
        let path = self.path_for(profile.name());
        if !force && path.exists() {
            return Err(ProfileError::AlreadyExists(profile.title.clone()));
        }
        let yaml = profile.to_yaml()?;
        std::fs::write(&path, yaml)?;
        Ok(())
    }

    /// Remove a profile by name.
    pub fn remove(&self, name: &str) -> ProfileResult<()> {
        let path = self.path_for(name);
        if !path.exists() {
            return Err(ProfileError::NotFound(name.to_string()));
        }
        std::fs::remove_file(&path)?;
        Ok(())
    }

    /// Install bundled default profiles (skips existing ones unless `force` is true).
    pub fn install_defaults(&self, force: bool) -> ProfileResult<usize> {
        self.ensure_dir()?;
        let mut count = 0;
        for profile in all_defaults() {
            let path = self.path_for(profile.name());
            if !force && path.exists() {
                continue;
            }
            let yaml = profile.to_yaml()?;
            std::fs::write(&path, yaml)?;
            count += 1;
        }
        Ok(count)
    }

    /// Resolve the file path for a profile name.
    fn path_for(&self, name: &str) -> PathBuf {
        let file_name = format!("{}.yaml", name.to_lowercase().replace(' ', "-"));
        self.dir.join(file_name)
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
}
