use std::path::{Path, PathBuf};

use crate::defaults::all_defaults;
use crate::error::{TeamError, TeamResult};
use crate::team::TeamDefinition;

/// CRUD store for team definitions on disk (`~/.opengoose/teams/`).
pub struct TeamStore {
    dir: PathBuf,
}

impl TeamStore {
    /// Create a store backed by `~/.opengoose/teams/`.
    pub fn new() -> TeamResult<Self> {
        let home = dirs::home_dir().ok_or(TeamError::NoHomeDir)?;
        Ok(Self {
            dir: home.join(".opengoose").join("teams"),
        })
    }

    /// Create a store backed by a custom directory (for testing).
    pub fn with_dir(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The teams directory path.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Ensure the teams directory exists.
    fn ensure_dir(&self) -> TeamResult<()> {
        std::fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    /// List all team names (sorted).
    pub fn list(&self) -> TeamResult<Vec<String>> {
        if !self.dir.exists() {
            return Ok(vec![]);
        }

        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "yaml" || ext == "yml") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(team) = TeamDefinition::from_yaml(&content) {
                        names.push(team.title);
                    }
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// Get a team by name.
    pub fn get(&self, name: &str) -> TeamResult<TeamDefinition> {
        let path = self.path_for(name);
        if !path.exists() {
            return Err(TeamError::NotFound(name.to_string()));
        }
        let content = std::fs::read_to_string(&path)?;
        TeamDefinition::from_yaml(&content)
    }

    /// Save a team. If `force` is false and the file exists, returns `AlreadyExists`.
    pub fn save(&self, team: &TeamDefinition, force: bool) -> TeamResult<()> {
        self.ensure_dir()?;
        let path = self.path_for(team.name());
        if !force && path.exists() {
            return Err(TeamError::AlreadyExists(team.title.clone()));
        }
        let yaml = team.to_yaml()?;
        std::fs::write(&path, yaml)?;
        Ok(())
    }

    /// Remove a team by name.
    pub fn remove(&self, name: &str) -> TeamResult<()> {
        let path = self.path_for(name);
        if !path.exists() {
            return Err(TeamError::NotFound(name.to_string()));
        }
        std::fs::remove_file(&path)?;
        Ok(())
    }

    /// Install bundled default teams (skips existing ones unless `force` is true).
    pub fn install_defaults(&self, force: bool) -> TeamResult<usize> {
        self.ensure_dir()?;
        let mut count = 0;
        for team in all_defaults() {
            let path = self.path_for(team.name());
            if !force && path.exists() {
                continue;
            }
            let yaml = team.to_yaml()?;
            std::fs::write(&path, yaml)?;
            count += 1;
        }
        Ok(count)
    }

    /// Resolve the file path for a team name.
    fn path_for(&self, name: &str) -> PathBuf {
        let file_name = format!("{}.yaml", name.to_lowercase().replace(' ', "-"));
        self.dir.join(file_name)
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
}
