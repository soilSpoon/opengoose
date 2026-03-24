use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const LOCK_VERSION: u64 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLockEntry {
    pub source: String,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceUrl")]
    pub source_url: String,
    #[serde(rename = "skillPath", default)]
    pub skill_path: Option<String>,
    #[serde(rename = "skillFolderHash", default)]
    pub skill_folder_hash: String,
    #[serde(rename = "installedAt")]
    pub installed_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(
        rename = "pluginName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub plugin_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillLockFile {
    pub version: u64,
    pub skills: HashMap<String, SkillLockEntry>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub fn lock_path(base_dir: &Path) -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        PathBuf::from(xdg).join("skills").join(".skill-lock.json")
    } else {
        base_dir.join(".agents").join(".skill-lock.json")
    }
}

pub fn read_lock(base_dir: &Path) -> SkillLockFile {
    let path = lock_path(base_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<SkillLockFile>(&content) {
            Ok(lock) if lock.version >= LOCK_VERSION => lock,
            _ => empty_lock(),
        },
        Err(_) => empty_lock(),
    }
}

pub fn write_lock(base_dir: &Path, lock: &SkillLockFile) -> anyhow::Result<()> {
    let path = lock_path(base_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(lock)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn add_entry(base_dir: &Path, name: &str, entry: SkillLockEntry) -> anyhow::Result<()> {
    let mut lock = read_lock(base_dir);
    lock.skills.insert(name.to_string(), entry);
    write_lock(base_dir, &lock)
}

pub fn remove_entry(base_dir: &Path, name: &str) -> anyhow::Result<bool> {
    let mut lock = read_lock(base_dir);
    let removed = lock.skills.remove(name).is_some();
    if removed {
        write_lock(base_dir, &lock)?;
    }
    Ok(removed)
}

/// ISO 8601 timestamp. chrono is already a workspace dependency.
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn empty_lock() -> SkillLockFile {
    SkillLockFile {
        version: LOCK_VERSION,
        skills: HashMap::new(),
        extra: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::IsolatedEnv;

    #[test]
    fn empty_lock_has_version_3() {
        let lock = empty_lock();
        assert_eq!(lock.version, 3);
        assert!(lock.skills.is_empty());
    }

    #[test]
    fn roundtrip_lock_file() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        let base = tmp.path();
        let mut lock = empty_lock();
        lock.skills.insert(
            "test-skill".to_string(),
            SkillLockEntry {
                source: "owner/repo".to_string(),
                source_type: "github".to_string(),
                source_url: "https://github.com/owner/repo.git".to_string(),
                skill_path: Some("skills/test-skill".to_string()),
                skill_folder_hash: "abc123".to_string(),
                installed_at: "2026-03-19T10:00:00Z".to_string(),
                updated_at: "2026-03-19T10:00:00Z".to_string(),
                plugin_name: None,
            },
        );
        write_lock(base, &lock).expect("lock operation should succeed");

        let loaded = read_lock(base);
        assert_eq!(loaded.skills.len(), 1);
        assert_eq!(loaded.skills["test-skill"].source, "owner/repo");
    }

    #[test]
    fn now_iso_returns_iso8601_string() {
        let ts = now_iso();
        assert!(ts.contains('T'), "expected ISO 8601 format, got: {ts}");
        chrono::DateTime::parse_from_rfc3339(&ts).expect("should parse as RFC3339");
    }

    #[test]
    fn remove_entry_returns_false_when_not_present() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        let removed = remove_entry(tmp.path(), "nonexistent").expect("operation should succeed");
        assert!(!removed);
    }

    #[test]
    fn read_lock_returns_empty_for_invalid_json() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        // Write invalid JSON to the lock path location
        let state_dir = tmp.path().join("xdg/skills");
        std::fs::create_dir_all(&state_dir).expect("directory creation should succeed");
        std::fs::write(state_dir.join(".skill-lock.json"), "not valid json")
            .expect("test fixture write should succeed");

        let lock = read_lock(tmp.path());
        assert!(lock.skills.is_empty());
    }

    #[test]
    fn lock_path_uses_xdg_state_home_when_set() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        // IsolatedEnv sets XDG_STATE_HOME to tmp/xdg
        let path = lock_path(tmp.path());
        assert!(path.starts_with(tmp.path().join("xdg")));
    }

    #[test]
    fn preserves_extra_fields() {
        let json = r#"{
            "version": 3,
            "skills": {},
            "dismissed": {"findSkillsPrompt": true},
            "lastSelectedAgents": ["claude-code"]
        }"#;
        let lock: SkillLockFile = serde_json::from_str(json).expect("test JSON should parse");
        assert!(lock.extra.contains_key("dismissed"));
        assert!(lock.extra.contains_key("lastSelectedAgents"));

        let serialized = serde_json::to_string(&lock).expect("JSON serialization should succeed");
        assert!(serialized.contains("dismissed"));
        assert!(serialized.contains("lastSelectedAgents"));
    }

    #[test]
    fn add_entry_and_read_back() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        let entry = SkillLockEntry {
            source: "owner/repo".to_string(),
            source_type: "github".to_string(),
            source_url: "https://github.com/owner/repo.git".to_string(),
            skill_path: None,
            skill_folder_hash: "hash123".to_string(),
            installed_at: now_iso(),
            updated_at: now_iso(),
            plugin_name: None,
        };
        add_entry(tmp.path(), "added-skill", entry).expect("operation should succeed");

        let loaded = read_lock(tmp.path());
        assert!(loaded.skills.contains_key("added-skill"));
        assert_eq!(loaded.skills["added-skill"].source, "owner/repo");
    }

    #[test]
    fn lock_path_falls_back_without_xdg() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        // Use base_dir directly without XDG override
        // We need to unset XDG_STATE_HOME temporarily
        let prev_xdg = std::env::var_os("XDG_STATE_HOME");
        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
        }

        let path = lock_path(tmp.path());
        assert!(path.to_string_lossy().contains(".agents"));

        unsafe {
            match prev_xdg {
                Some(p) => std::env::set_var("XDG_STATE_HOME", p),
                None => std::env::remove_var("XDG_STATE_HOME"),
            }
        }
    }

    #[test]
    fn read_lock_returns_empty_for_old_version() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let _env = IsolatedEnv::new(tmp.path());

        let state_dir = tmp.path().join("xdg/skills");
        std::fs::create_dir_all(&state_dir).expect("directory creation should succeed");
        // Write a valid JSON but with old version (1 < LOCK_VERSION=3)
        std::fs::write(
            state_dir.join(".skill-lock.json"),
            r#"{"version": 1, "skills": {}}"#,
        )
        .expect("operation should succeed");

        let lock = read_lock(tmp.path());
        assert!(
            lock.skills.is_empty(),
            "old version should yield empty lock"
        );
    }
}
