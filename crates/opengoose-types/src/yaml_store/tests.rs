use std::path::{Path, PathBuf};

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
fn test_list_ignores_non_yaml_files() {
    // list() only considers files with .yaml or .yml extension.
    let tmp = tempfile::tempdir().unwrap();
    let store = YamlFileStore::new(tmp.path().to_path_buf());

    // Write a valid definition through the store (gets a .yaml extension).
    store.save(&test_def("alpha", "1"), false).unwrap();

    // Write files with other extensions directly — these must be skipped.
    std::fs::write(
        tmp.path().join("README.txt"),
        "name: should-be-ignored\nvalue: x\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("config.json"),
        "name: also-ignored\nvalue: y\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join("no-extension"), "name: no-ext\nvalue: z\n").unwrap();

    let names = store.list::<TestDef>().unwrap();
    assert_eq!(names, vec!["alpha"]);
}

#[test]
fn test_list_ignores_invalid_yaml_files() {
    // list() silently skips .yaml files that fail to parse as valid definitions.
    let tmp = tempfile::tempdir().unwrap();
    let store = YamlFileStore::new(tmp.path().to_path_buf());

    // Write a valid item.
    store.save(&test_def("valid", "ok"), false).unwrap();

    // Write a .yaml file that will fail TestDef::from_yaml (missing "name" field).
    std::fs::write(
        tmp.path().join("broken.yaml"),
        "value: only-value-no-name\n",
    )
    .unwrap();

    let names = store.list::<TestDef>().unwrap();
    assert_eq!(names, vec!["valid"]);
}

#[test]
fn test_read_cached_returns_cached_on_same_mtime() {
    // Exercise the cache-hit path: a second `get()` for the same file,
    // without any modification in between, should return the cached content.
    let tmp = tempfile::tempdir().unwrap();
    let store = YamlFileStore::new(tmp.path().to_path_buf());
    let item = test_def("cached-item", "v1");
    store.save(&item, false).unwrap();

    // First read → cache miss, populates cache.
    let first: TestDef = store.get("cached-item").unwrap();
    assert_eq!(first.value, "v1");

    // Second read → cache hit (mtime unchanged).
    let second: TestDef = store.get("cached-item").unwrap();
    assert_eq!(second.value, "v1");
}

#[test]
fn test_cache_invalidated_after_save() {
    // After `save(force=true)`, the cache entry is evicted, so the next
    // `get()` must re-read from disk and see the updated content.
    let tmp = tempfile::tempdir().unwrap();
    let store = YamlFileStore::new(tmp.path().to_path_buf());

    store.save(&test_def("item", "v1"), false).unwrap();
    let first: TestDef = store.get("item").unwrap();
    assert_eq!(first.value, "v1");

    // Overwrite via force-save.
    store.save(&test_def("item", "v2"), true).unwrap();
    let second: TestDef = store.get("item").unwrap();
    assert_eq!(second.value, "v2");
}

#[test]
fn test_list_includes_yml_extension() {
    // list() accepts both .yaml and .yml extensions.
    let tmp = tempfile::tempdir().unwrap();
    let store = YamlFileStore::new(tmp.path().to_path_buf());

    store.save(&test_def("via-store", "1"), false).unwrap();
    // Write a valid item with .yml extension directly.
    std::fs::write(
        tmp.path().join("shortform.yml"),
        "name: shortform\nvalue: 2\n",
    )
    .unwrap();

    let mut names = store.list::<TestDef>().unwrap();
    names.sort();
    assert_eq!(names, vec!["shortform", "via-store"]);
}

#[test]
fn test_ensure_dir_rejects_parent_dir_components() {
    let store = YamlFileStore::new(PathBuf::from("../unsafe-store"));
    let err = store.ensure_dir().unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn test_get_rejects_nested_store_path() {
    let store = YamlFileStore::new(PathBuf::from("/tmp/store/nested"));
    let err = store.validate_store_path(Path::new("/tmp/store/nested/inner/item.yaml"));
    assert!(err.is_err());
}
