use std::path::Path;

use super::DiscoveredSkill;
use super::parse::parse_skill_md;

pub(super) fn scan_dir(
    dir: &Path,
    repo_root: &Path,
    skills: &mut Vec<DiscoveredSkill>,
    depth: usize,
) {
    if depth > 5 {
        return;
    }

    let skill_md = dir.join("SKILL.md");
    if skill_md.is_file()
        && let Some(skill) = parse_skill_md(&skill_md, dir, repo_root)
    {
        skills.push(skill);
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.')
            || name_str == "node_modules"
            || name_str == "target"
            || name_str == "__pycache__"
        {
            continue;
        }
        scan_dir(&path, repo_root, skills, depth + 1);
    }
}
