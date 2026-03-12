mod validation;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use dashmap::DashMap;

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
///
/// Each individual file read is cached in memory and invalidated
/// automatically when the file's last-modified timestamp changes,
/// providing hot-reload behaviour with zero extra I/O on cache hits.
///
/// `Clone` is cheap: clones share the same underlying `file_cache` `Arc`,
/// so all copies benefit from each other's reads automatically.
#[derive(Clone)]
pub struct YamlFileStore {
    dir: PathBuf,
    /// Per-file content cache: path → (raw YAML string, last-modified time).
    file_cache: Arc<DashMap<PathBuf, (String, SystemTime)>>,
}

impl YamlFileStore {
    /// Create a store backed by the given directory.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            file_cache: Arc::new(DashMap::new()),
        }
    }

    /// The directory path this store operates on.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Ensure the backing directory exists.
    pub fn ensure_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.validated_dir()?)
    }

    /// List all definition names (sorted) by scanning YAML files.
    pub fn list<T: YamlDefinition>(&self) -> Result<Vec<String>, T::Error> {
        let dir = self.validated_dir()?;
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut names = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            self.validate_store_path(&path)?;
            if path
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(item) = T::from_yaml(&content)
            {
                names.push(item.title().to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    /// Get a definition by name.
    ///
    /// Reads through the in-memory file cache; disk access only occurs
    /// when the file has changed since the last read (mtime-based hot reload).
    pub fn get<T: YamlDefinition>(&self, name: &str) -> Result<T, T::Error> {
        let path = self.store_path_for(name)?;
        let content = self.read_cached(&path)?;
        T::from_yaml(&content)
    }

    /// Save a definition. If `force` is false and the file exists,
    /// returns `Err` with an `io::ErrorKind::AlreadyExists` error.
    pub fn save<T: YamlDefinition>(&self, item: &T, force: bool) -> Result<(), T::Error> {
        self.ensure_dir()?;
        let path = self.store_path_for(item.title())?;
        if !force && path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("'{}' already exists", item.title()),
            )
            .into());
        }
        let yaml = item.to_yaml()?;
        std::fs::write(&path, yaml)?;
        self.invalidate(&path);
        Ok(())
    }

    /// Remove a definition by name.
    pub fn remove(&self, name: &str) -> std::io::Result<()> {
        let path = self.store_path_for(name)?;
        if !path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("'{name}' not found"),
            ));
        }
        std::fs::remove_file(&path)?;
        self.invalidate(&path);
        Ok(())
    }

    /// Resolve the file path for a definition name.
    ///
    /// Uses `sanitize_name` to prevent path traversal.
    pub fn path_for(&self, name: &str) -> PathBuf {
        let sanitized = crate::sanitize_name(&name.to_lowercase().replace(' ', "-"));
        self.dir.join(format!("{sanitized}.yaml"))
    }

    /// Read a file through the in-memory cache.
    ///
    /// Returns cached content when the file's mtime matches; re-reads
    /// from disk and updates the cache otherwise.
    fn read_cached(&self, path: &Path) -> std::io::Result<String> {
        self.validate_store_path(path)?;
        let mtime = std::fs::metadata(path)?.modified()?;

        // Fast path: valid cache entry.
        if let Some(cached) = self.file_cache.get(path)
            && cached.value().1 == mtime
        {
            return Ok(cached.value().0.clone());
        }

        // Cache miss or stale — read from disk and update.
        let content = std::fs::read_to_string(path)?;
        self.file_cache
            .insert(path.to_path_buf(), (content.clone(), mtime));
        Ok(content)
    }

    /// Evict a single file from the cache (called after writes/deletes).
    fn invalidate(&self, path: &Path) {
        self.file_cache.remove(path);
    }
}
