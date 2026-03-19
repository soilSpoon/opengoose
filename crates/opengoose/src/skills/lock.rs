use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    #[serde(rename = "pluginName", default, skip_serializing_if = "Option::is_none")]
    pub plugin_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillLockFile {
    pub version: u64,
    pub skills: HashMap<String, SkillLockEntry>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub fn read_lock() -> SkillLockFile {
    todo!()
}

pub fn write_lock(_lock: &SkillLockFile) -> anyhow::Result<()> {
    todo!()
}

pub fn add_entry(_name: &str, _entry: SkillLockEntry) -> anyhow::Result<()> {
    todo!()
}

pub fn remove_entry(_name: &str) -> anyhow::Result<bool> {
    todo!()
}

pub fn now_iso() -> String {
    todo!()
}
