use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: String,
    pub rel_path: String,
    pub abs_path: PathBuf,
}

pub fn discover_skills(_repo_path: &Path) -> Vec<DiscoveredSkill> {
    todo!()
}
