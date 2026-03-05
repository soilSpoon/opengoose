use std::path::{Path, PathBuf};

/// Trait for YAML-serializable definitions (profiles, teams, etc.)
///
/// Implementors provide YAML parsing/serialization and validation logic.
/// The `YamlFileStore` uses this trait for generic CRUD operations.
pub trait YamlDefinition: Sized {
    /// The error type for parsing/validation/serialization.
    ///
    /// Must convert from `std::io::Error` so file operations can use `?`.
    type Error: From<std::io::Error>;

    /// The display name / title of this definition.
    fn title(&self) -> &str;

    /// Parse from a YAML string, including validation.
    fn from_yaml(yaml: &str) -> Result<Self, Self::Error>;

    /// Serialize to a YAML string.
    fn to_yaml(&self) -> Result<String, Self::Error>;

    /// File-safe name derived from the title.
    fn file_name(&self) -> String {
        format!(
            "{}.yaml",
            crate::sanitize_name(&self.title().to_lowercase().replace(' ', "-"))
        )
    }
}

/// Generic YAML-file-backed CRUD store.
///
/// Provides common file operations for any `YamlDefinition` type.
/// Crate-specific stores (ProfileStore, TeamStore) wrap this with
/// their own error handling and convenience methods.
pub struct YamlFileStore {
    dir: PathBuf,
}

impl YamlFileStore {
    /// Create a store backed by the given directory.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The directory path this store operates on.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Ensure the backing directory exists.
    pub fn ensure_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)
    }

    /// List all definition names (sorted) by scanning YAML files.
    pub fn list<T: YamlDefinition>(&self) -> Result<Vec<String>, T::Error> {
        if !self.dir.exists() {
            return Ok(vec![]);
        }

        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(item) = T::from_yaml(&content) {
                        names.push(item.title().to_string());
                    }
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// Get a definition by name.
    pub fn get<T: YamlDefinition>(&self, name: &str) -> Result<T, T::Error> {
        let path = self.path_for(name);
        let content = std::fs::read_to_string(&path)?;
        T::from_yaml(&content)
    }

    /// Save a definition. If `force` is false and the file exists,
    /// returns `Err` with an `io::ErrorKind::AlreadyExists` error.
    pub fn save<T: YamlDefinition>(&self, item: &T, force: bool) -> Result<(), T::Error> {
        self.ensure_dir()?;
        let path = self.path_for(item.title());
        if !force && path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("'{}' already exists", item.title()),
            )
            .into());
        }
        let yaml = item.to_yaml()?;
        std::fs::write(&path, yaml)?;
        Ok(())
    }

    /// Remove a definition by name.
    pub fn remove(&self, name: &str) -> std::io::Result<()> {
        let path = self.path_for(name);
        if !path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("'{name}' not found"),
            ));
        }
        std::fs::remove_file(&path)
    }

    /// Resolve the file path for a definition name.
    ///
    /// Uses `sanitize_name` to prevent path traversal.
    pub fn path_for(&self, name: &str) -> PathBuf {
        let sanitized = crate::sanitize_name(&name.to_lowercase().replace(' ', "-"));
        self.dir.join(format!("{sanitized}.yaml"))
    }
}
