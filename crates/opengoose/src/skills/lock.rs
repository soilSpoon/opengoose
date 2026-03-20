use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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

pub fn lock_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        PathBuf::from(xdg).join("skills").join(".skill-lock.json")
    } else {
        crate::home_dir().join(".agents").join(".skill-lock.json")
    }
}

pub fn read_lock() -> SkillLockFile {
    let path = lock_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<SkillLockFile>(&content) {
            Ok(lock) if lock.version >= LOCK_VERSION => lock,
            _ => empty_lock(),
        },
        Err(_) => empty_lock(),
    }
}

pub fn write_lock(lock: &SkillLockFile) -> anyhow::Result<()> {
    let path = lock_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(lock)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn add_entry(name: &str, entry: SkillLockEntry) -> anyhow::Result<()> {
    let mut lock = read_lock();
    lock.skills.insert(name.to_string(), entry);
    write_lock(&lock)
}

pub fn remove_entry(name: &str) -> anyhow::Result<bool> {
    let mut lock = read_lock();
    let removed = lock.skills.remove(name).is_some();
    if removed {
        write_lock(&lock)?;
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

    #[test]
    fn empty_lock_has_version_3() {
        let lock = empty_lock();
        assert_eq!(lock.version, 3);
        assert!(lock.skills.is_empty());
    }

    #[test]
    fn roundtrip_lock_file() {
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));
        }

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
        write_lock(&lock).unwrap();

        let loaded = read_lock();
        assert_eq!(loaded.skills.len(), 1);
        assert_eq!(loaded.skills["test-skill"].source, "owner/repo");

        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
        }
    }

    #[test]
    fn preserves_extra_fields() {
        let json = r#"{
            "version": 3,
            "skills": {},
            "dismissed": {"findSkillsPrompt": true},
            "lastSelectedAgents": ["claude-code"]
        }"#;
        let lock: SkillLockFile = serde_json::from_str(json).unwrap();
        assert!(lock.extra.contains_key("dismissed"));
        assert!(lock.extra.contains_key("lastSelectedAgents"));

        let serialized = serde_json::to_string(&lock).unwrap();
        assert!(serialized.contains("dismissed"));
        assert!(serialized.contains("lastSelectedAgents"));
    }
}
