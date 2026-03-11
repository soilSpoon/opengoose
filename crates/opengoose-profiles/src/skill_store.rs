use std::path::{Path, PathBuf};

use opengoose_types::YamlFileStore;

use crate::error::{ProfileError, ProfileResult};
use crate::skill::Skill;
use crate::skill_defaults::all_default_skills;

/// CRUD store for skills on disk (`~/.opengoose/skills/`).
///
/// Skills are YAML packages bundling named extension sets. Profiles reference
/// skills by name; when resolved, duplicate extensions (same name) are
/// deduplicated — the first occurrence wins.
///
/// `Clone` is cheap: the underlying `YamlFileStore` shares its file cache via
/// an `Arc`, so all clones benefit from cache hits populated by any copy.
#[derive(Clone)]
pub struct SkillStore {
    inner: YamlFileStore,
}

impl SkillStore {
    /// Create a store backed by `~/.opengoose/skills/`.
    pub fn new() -> ProfileResult<Self> {
        let home = dirs::home_dir().ok_or(opengoose_types::YamlStoreError::NoHomeDir)?;
        Ok(Self {
            inner: YamlFileStore::new(home.join(".opengoose").join("skills")),
        })
    }

    /// Create a store backed by a custom directory (for testing).
    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            inner: YamlFileStore::new(dir),
        }
    }

    /// The skills directory path.
    pub fn dir(&self) -> &Path {
        self.inner.dir()
    }

    /// List all skill names (sorted).
    pub fn list(&self) -> ProfileResult<Vec<String>> {
        self.inner.list::<Skill>()
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> ProfileResult<Skill> {
        self.inner.get::<Skill>(name).map_err(|e| {
            if let ProfileError::Store(opengoose_types::YamlStoreError::Io(ref io_err)) = e
                && io_err.kind() == std::io::ErrorKind::NotFound
            {
                return ProfileError::SkillNotFound(name.to_string());
            }
            e
        })
    }

    /// Save a skill. If `force` is false and the file exists, returns `AlreadyExists`.
    pub fn save(&self, skill: &Skill, force: bool) -> ProfileResult<()> {
        self.inner.save(skill, force).map_err(|e| {
            if let ProfileError::Store(opengoose_types::YamlStoreError::Io(ref io_err)) = e
                && io_err.kind() == std::io::ErrorKind::AlreadyExists
            {
                return ProfileError::SkillAlreadyExists(skill.name.clone());
            }
            e
        })
    }

    /// Remove a skill by name.
    pub fn remove(&self, name: &str) -> ProfileResult<()> {
        self.inner.remove(name).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProfileError::SkillNotFound(name.to_string())
            } else {
                ProfileError::Store(opengoose_types::YamlStoreError::Io(e))
            }
        })
    }

    /// Install bundled default skills (skips existing ones unless `force` is true).
    pub fn install_defaults(&self, force: bool) -> ProfileResult<usize> {
        self.inner.ensure_dir()?;
        let mut count = 0;
        for skill in all_default_skills() {
            let path = self.inner.path_for(skill.name.as_str());
            if !force && path.exists() {
                continue;
            }
            let yaml = skill.to_yaml()?;
            std::fs::write(&path, yaml)?;
            count += 1;
        }
        Ok(count)
    }

    /// Resolve a list of skill names to their combined extensions.
    ///
    /// Extensions are deduplicated by name — the first occurrence of each
    /// extension name wins (profile's own extensions take precedence).
    pub fn resolve_extensions(
        &self,
        skill_names: &[String],
    ) -> ProfileResult<Vec<crate::profile::ExtensionRef>> {
        let mut seen = std::collections::HashSet::new();
        let mut extensions = Vec::new();

        for name in skill_names {
            let skill = self.get(name)?;
            for ext in skill.extensions {
                if seen.insert(ext.name.clone()) {
                    extensions.push(ext);
                }
            }
        }
        Ok(extensions)
    }

    /// Resolve the absolute file path for a skill name.
    pub fn skill_path(&self, name: &str) -> String {
        self.inner.path_for(name).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ExtensionRef;

    fn temp_store() -> (tempfile::TempDir, SkillStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = SkillStore::with_dir(tmp.path().to_path_buf());
        (tmp, store)
    }

    fn make_skill(name: &str, ext_names: &[&str]) -> Skill {
        Skill {
            name: name.to_string(),
            description: None,
            version: "1.0.0".to_string(),
            extensions: ext_names
                .iter()
                .map(|n| ExtensionRef {
                    name: n.to_string(),
                    ext_type: "stdio".to_string(),
                    cmd: Some("echo".to_string()),
                    args: vec![],
                    uri: None,
                    timeout: None,
                    envs: Default::default(),
                    env_keys: vec![],
                    code: None,
                    dependencies: None,
                })
                .collect(),
        }
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
        assert_eq!(names, vec!["file-manager", "git-tools", "web-search"]);
    }

    #[test]
    fn get_and_remove() {
        let (_tmp, store) = temp_store();
        let skill = make_skill("my-skill", &["tool-a"]);
        store.save(&skill, false).unwrap();
        let loaded = store.get("my-skill").unwrap();
        assert_eq!(loaded.name, "my-skill");
        store.remove("my-skill").unwrap();
        assert!(store.get("my-skill").is_err());
    }

    #[test]
    fn get_not_found_typed_error() {
        let (_tmp, store) = temp_store();
        let err = store.get("nonexistent").unwrap_err();
        assert!(matches!(err, ProfileError::SkillNotFound(_)));
    }

    #[test]
    fn save_no_force_rejects_duplicate() {
        let (_tmp, store) = temp_store();
        let skill = make_skill("dup", &[]);
        store.save(&skill, false).unwrap();
        let err = store.save(&skill, false).unwrap_err();
        assert!(matches!(err, ProfileError::SkillAlreadyExists(_)));
    }

    #[test]
    fn resolve_extensions_deduplicates() {
        let (_tmp, store) = temp_store();
        let s1 = make_skill("skill-a", &["tool-x", "tool-y"]);
        let s2 = make_skill("skill-b", &["tool-y", "tool-z"]);
        store.save(&s1, false).unwrap();
        store.save(&s2, false).unwrap();

        let exts = store
            .resolve_extensions(&["skill-a".to_string(), "skill-b".to_string()])
            .unwrap();
        // tool-y appears in both skills; only the first occurrence (from skill-a) wins.
        assert_eq!(exts.len(), 3);
        let names: Vec<_> = exts.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["tool-x", "tool-y", "tool-z"]);
    }

    #[test]
    fn resolve_extensions_missing_skill_errors() {
        let (_tmp, store) = temp_store();
        let err = store
            .resolve_extensions(&["missing".to_string()])
            .unwrap_err();
        assert!(matches!(err, ProfileError::SkillNotFound(_)));
    }
}
