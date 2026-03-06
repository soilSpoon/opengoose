use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

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
    file_cache: Arc<RwLock<HashMap<PathBuf, (String, SystemTime)>>>,
}

impl YamlFileStore {
    /// Create a store backed by the given directory.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            file_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Read a file through the in-memory cache.
    ///
    /// Returns cached content when the file's mtime matches; re-reads
    /// from disk and updates the cache otherwise.
    fn read_cached(&self, path: &Path) -> std::io::Result<String> {
        let mtime = std::fs::metadata(path)?.modified()?;

        // Fast path: valid cache entry.
        if let Ok(cache) = self.file_cache.read()
            && let Some((content, cached_mtime)) = cache.get(path)
            && *cached_mtime == mtime
        {
            return Ok(content.clone());
        }

        // Cache miss or stale — read from disk and update.
        let content = std::fs::read_to_string(path)?;
        if let Ok(mut cache) = self.file_cache.write() {
            cache.insert(path.to_path_buf(), (content.clone(), mtime));
        }
        Ok(content)
    }

    /// Evict a single file from the cache (called after writes/deletes).
    fn invalidate(&self, path: &Path) {
        if let Ok(mut cache) = self.file_cache.write() {
            cache.remove(path);
        }
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
        let path = self.path_for(name);
        let content = self.read_cached(&path)?;
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
        self.invalidate(&path);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple test implementation of YamlDefinition.
    #[derive(Debug, Clone, PartialEq)]
    struct TestDef {
        name: String,
        value: String,
    }

    impl YamlDefinition for TestDef {
        type Error = std::io::Error;

        fn title(&self) -> &str {
            &self.name
        }

        fn from_yaml(yaml: &str) -> Result<Self, Self::Error> {
            let mut name = String::new();
            let mut value = String::new();
            for line in yaml.lines() {
                if let Some(rest) = line.strip_prefix("name: ") {
                    name = rest.to_string();
                } else if let Some(rest) = line.strip_prefix("value: ") {
                    value = rest.to_string();
                }
            }
            if name.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "missing name",
                ));
            }
            Ok(TestDef { name, value })
        }

        fn to_yaml(&self) -> Result<String, Self::Error> {
            Ok(format!("name: {}\nvalue: {}\n", self.name, self.value))
        }
    }

    fn test_def(name: &str, value: &str) -> TestDef {
        TestDef {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn test_new_and_dir() {
        let dir = PathBuf::from("/tmp/test-store");
        let store = YamlFileStore::new(dir.clone());
        assert_eq!(store.dir(), dir.as_path());
    }

    #[test]
    fn test_ensure_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a").join("b");
        let store = YamlFileStore::new(nested.clone());
        assert!(!nested.exists());
        store.ensure_dir().unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn test_path_for() {
        let store = YamlFileStore::new(PathBuf::from("/store"));
        assert_eq!(
            store.path_for("My Profile"),
            PathBuf::from("/store/my-profile.yaml")
        );
    }

    #[test]
    fn test_path_for_traversal() {
        let store = YamlFileStore::new(PathBuf::from("/store"));
        let path = store.path_for("../../etc/passwd");
        assert!(path.starts_with("/store"));
        assert!(!path.to_string_lossy().contains(".."));
    }

    #[test]
    fn test_file_name_trait_default() {
        let def = test_def("My Cool Profile", "v1");
        assert_eq!(def.file_name(), "my-cool-profile.yaml");
    }

    #[test]
    fn test_save_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        let item = test_def("alpha", "one");
        store.save(&item, false).unwrap();
        let loaded: TestDef = store.get("alpha").unwrap();
        assert_eq!(loaded, item);
    }

    #[test]
    fn test_list_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        let names = store.list::<TestDef>().unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn test_list_nonexistent_dir() {
        let store = YamlFileStore::new(PathBuf::from("/nonexistent/path/xyz"));
        let names = store.list::<TestDef>().unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn test_list_with_items() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        store.save(&test_def("charlie", "3"), false).unwrap();
        store.save(&test_def("alpha", "1"), false).unwrap();
        store.save(&test_def("bravo", "2"), false).unwrap();
        let names = store.list::<TestDef>().unwrap();
        assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn test_save_no_force_duplicate() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        store.save(&test_def("item", "v1"), false).unwrap();
        let err = store.save(&test_def("item", "v2"), false).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn test_save_force_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        store.save(&test_def("item", "v1"), false).unwrap();
        store.save(&test_def("item", "v2"), true).unwrap();
        let loaded: TestDef = store.get("item").unwrap();
        assert_eq!(loaded.value, "v2");
    }

    #[test]
    fn test_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        store.save(&test_def("item", "v1"), false).unwrap();
        store.remove("item").unwrap();
        assert!(store.get::<TestDef>("item").is_err());
    }

    #[test]
    fn test_remove_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        let err = store.remove("nonexistent").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_get_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        let err = store.get::<TestDef>("nonexistent").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_read_cached_returns_cached_on_second_read() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        store.save(&test_def("cached", "v1"), false).unwrap();
        // First read populates cache
        let first: TestDef = store.get("cached").unwrap();
        assert_eq!(first.value, "v1");
        // Second read hits cache (same mtime)
        let second: TestDef = store.get("cached").unwrap();
        assert_eq!(second.value, "v1");
        assert!(!store.file_cache.read().unwrap().is_empty());
    }

    #[test]
    fn test_invalidate_clears_cache_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        store.save(&test_def("item", "v1"), false).unwrap();
        let _: TestDef = store.get("item").unwrap();
        store.save(&test_def("item", "v2"), true).unwrap();
        let loaded: TestDef = store.get("item").unwrap();
        assert_eq!(loaded.value, "v2");
    }

    #[test]
    fn test_clone_shares_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        let store2 = store.clone();
        store.save(&test_def("shared", "v1"), false).unwrap();
        let _: TestDef = store.get("shared").unwrap();
        assert!(!store2.file_cache.read().unwrap().is_empty());
    }

    #[test]
    fn test_list_ignores_invalid_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        store.ensure_dir().unwrap();
        store.save(&test_def("good", "v1"), false).unwrap();
        std::fs::write(tmp.path().join("bad.yaml"), "not valid yaml").unwrap();
        let names = store.list::<TestDef>().unwrap();
        assert_eq!(names, vec!["good"]);
    }

    #[test]
    fn test_list_ignores_non_yaml_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = YamlFileStore::new(tmp.path().to_path_buf());
        store.ensure_dir().unwrap();
        store.save(&test_def("valid", "v1"), false).unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "not a yaml").unwrap();
        let names = store.list::<TestDef>().unwrap();
        assert_eq!(names, vec!["valid"]);
    }
}
