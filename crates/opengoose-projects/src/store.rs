use std::path::{Path, PathBuf};

use opengoose_types::YamlFileStore;

use crate::error::{ProjectError, ProjectResult};
use crate::project::ProjectDefinition;

/// CRUD store for project definitions on disk (`~/.opengoose/projects/`).
pub struct ProjectStore {
    inner: YamlFileStore,
}

impl ProjectStore {
    /// Create a store backed by `~/.opengoose/projects/`.
    pub fn new() -> ProjectResult<Self> {
        let home = dirs::home_dir().ok_or(opengoose_types::YamlStoreError::NoHomeDir)?;
        Ok(Self {
            inner: YamlFileStore::new(home.join(".opengoose").join("projects")),
        })
    }

    /// Create a store backed by a custom directory (useful for testing).
    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            inner: YamlFileStore::new(dir),
        }
    }

    /// The projects directory path.
    pub fn dir(&self) -> &Path {
        self.inner.dir()
    }

    /// List all project names (sorted).
    pub fn list(&self) -> ProjectResult<Vec<String>> {
        self.inner.list::<ProjectDefinition>()
    }

    /// Get a project by name.
    pub fn get(&self, name: &str) -> ProjectResult<ProjectDefinition> {
        self.inner
            .get::<ProjectDefinition>(name)
            .map_err(|e| {
                if let ProjectError::Store(opengoose_types::YamlStoreError::Io(ref io_err)) = e
                    && io_err.kind() == std::io::ErrorKind::NotFound
                {
                    return ProjectError::NotFound(name.to_string());
                }
                e
            })
    }

    /// Save a project. If `force` is false and the file exists, returns
    /// `AlreadyExists`.
    pub fn save(&self, project: &ProjectDefinition, force: bool) -> ProjectResult<()> {
        self.inner.save(project, force).map_err(|e| {
            if let ProjectError::Store(opengoose_types::YamlStoreError::Io(ref io_err)) = e
                && io_err.kind() == std::io::ErrorKind::AlreadyExists
            {
                return ProjectError::AlreadyExists(project.title.clone());
            }
            e
        })
    }

    /// Add a project from a YAML file at the given path.
    ///
    /// Reads the file, parses it, and saves it to the store using the
    /// project's title as the file name. Returns an error if the file
    /// cannot be read or if a project with the same name already exists
    /// (unless `force` is true).
    pub fn add_from_path(&self, path: &Path, force: bool) -> ProjectResult<ProjectDefinition> {
        let yaml = std::fs::read_to_string(path)?;
        let project = ProjectDefinition::from_yaml(&yaml)?;
        self.save(&project, force)?;
        Ok(project)
    }

    /// Remove a project by name.
    pub fn remove(&self, name: &str) -> ProjectResult<()> {
        self.inner.remove(name).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProjectError::NotFound(name.to_string())
            } else {
                ProjectError::Store(opengoose_types::YamlStoreError::Io(e))
            }
        })
    }

    /// Resolve the file path for a project name.
    pub fn path_for(&self, name: &str) -> PathBuf {
        self.inner.path_for(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, ProjectStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::with_dir(tmp.path().to_path_buf());
        (tmp, store)
    }

    fn sample_project(title: &str) -> ProjectDefinition {
        ProjectDefinition {
            version: "1.0.0".into(),
            title: title.to_string(),
            goal: Some(format!("Goal for {title}")),
            cwd: None,
            context_files: vec![],
            default_team: None,
            description: None,
            settings: None,
        }
    }

    #[test]
    fn list_empty() {
        let (_tmp, store) = temp_store();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn save_and_get() {
        let (_tmp, store) = temp_store();
        let project = sample_project("my-project");
        store.save(&project, false).unwrap();

        let loaded = store.get("my-project").unwrap();
        assert_eq!(loaded.title, "my-project");
        assert_eq!(loaded.goal.as_deref(), Some("Goal for my-project"));
    }

    #[test]
    fn save_no_force_rejects_duplicate() {
        let (_tmp, store) = temp_store();
        let project = sample_project("dup");
        store.save(&project, false).unwrap();
        let err = store.save(&project, false).unwrap_err();
        assert!(matches!(err, ProjectError::AlreadyExists(_)));
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn save_force_overwrites() {
        let (_tmp, store) = temp_store();
        let mut project = sample_project("overwrite-me");
        store.save(&project, false).unwrap();

        project.goal = Some("Updated goal".into());
        store.save(&project, true).unwrap();

        let loaded = store.get("overwrite-me").unwrap();
        assert_eq!(loaded.goal.as_deref(), Some("Updated goal"));
    }

    #[test]
    fn remove_existing() {
        let (_tmp, store) = temp_store();
        store.save(&sample_project("removable"), false).unwrap();
        store.remove("removable").unwrap();
        assert!(matches!(
            store.get("removable").unwrap_err(),
            ProjectError::NotFound(_)
        ));
    }

    #[test]
    fn remove_not_found() {
        let (_tmp, store) = temp_store();
        let err = store.remove("ghost").unwrap_err();
        assert!(matches!(err, ProjectError::NotFound(_)));
    }

    #[test]
    fn list_multiple_sorted() {
        let (_tmp, store) = temp_store();
        store.save(&sample_project("charlie"), false).unwrap();
        store.save(&sample_project("alpha"), false).unwrap();
        store.save(&sample_project("bravo"), false).unwrap();

        let names = store.list().unwrap();
        assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn add_from_path() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ProjectStore::with_dir(tmp.path().to_path_buf());

        let yaml = r#"
version: "1.0.0"
title: "from-file"
goal: "Goal from file"
"#;
        let file_path = tmp.path().join("external.yaml");
        std::fs::write(&file_path, yaml).unwrap();

        let project = store.add_from_path(&file_path, false).unwrap();
        assert_eq!(project.title, "from-file");

        let loaded = store.get("from-file").unwrap();
        assert_eq!(loaded.goal.as_deref(), Some("Goal from file"));
    }

    #[test]
    fn get_not_found_returns_typed_error() {
        let (_tmp, store) = temp_store();
        let err = store.get("nonexistent").unwrap_err();
        assert!(matches!(err, ProjectError::NotFound(_)));
        assert!(err.to_string().contains("not found"));
    }
}
