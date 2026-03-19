use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A discovered skill from a repo.
#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: String,
    pub rel_path: String,
    pub abs_path: PathBuf,
}

#[derive(Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Discover all SKILL.md files in a cloned repo.
pub fn discover_skills(repo_path: &Path) -> Vec<DiscoveredSkill> {
    let mut skills = Vec::new();
    let standard_dirs = [
        "",
        "skills",
        ".agents/skills",
        ".goose/skills",
        ".claude/skills",
        ".opengoose/skills/installed",
        ".opengoose/skills/learned",
    ];

    for dir in &standard_dirs {
        let search_path = if dir.is_empty() {
            repo_path.to_path_buf()
        } else {
            repo_path.join(dir)
        };
        if search_path.is_dir() {
            scan_dir(&search_path, repo_path, &mut skills, 0);
        }
    }

    let mut seen = std::collections::HashSet::new();
    skills.retain(|s| seen.insert(s.name.clone()));
    skills
}

fn scan_dir(dir: &Path, repo_root: &Path, skills: &mut Vec<DiscoveredSkill>, depth: usize) {
    if depth > 5 {
        return;
    }

    let skill_md = dir.join("SKILL.md");
    if skill_md.is_file() {
        if let Some(skill) = parse_skill_md(&skill_md, dir, repo_root) {
            skills.push(skill);
        }
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

fn parse_skill_md(path: &Path, skill_dir: &Path, repo_root: &Path) -> Option<DiscoveredSkill> {
    let content = std::fs::read_to_string(path).ok()?;
    let frontmatter = extract_frontmatter(&content)?;
    let fm: SkillFrontmatter = serde_yaml_or_fallback(&frontmatter)?;

    let description = fm.description.filter(|d| !d.is_empty())?;

    if let Some(meta) = &fm.metadata {
        if let Some(serde_json::Value::Bool(true)) = meta.get("internal") {
            if std::env::var("INSTALL_INTERNAL_SKILLS").unwrap_or_default() != "1" {
                return None;
            }
        }
    }

    let dir_name = skill_dir.file_name().map(|n| n.to_string_lossy().to_string());
    let name = fm.name.unwrap_or_else(|| {
        dir_name.clone().unwrap_or_else(|| "unnamed".to_string())
    });

    if let Some(ref dn) = dir_name {
        if *dn != name && !dn.is_empty() {
            eprintln!("warning: skill name '{}' doesn't match directory '{}'", name, dn);
        }
    }

    let rel_path = skill_dir
        .strip_prefix(repo_root)
        .unwrap_or(skill_dir)
        .to_string_lossy()
        .to_string();

    Some(DiscoveredSkill { name, description, rel_path, abs_path: skill_dir.to_path_buf() })
}

fn extract_frontmatter(content: &str) -> Option<String> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    Some(rest[..end].to_string())
}

fn serde_yaml_or_fallback(yaml_str: &str) -> Option<SkillFrontmatter> {
    if let Ok(fm) = serde_json::from_value::<SkillFrontmatter>(yaml_to_json(yaml_str)) {
        return Some(fm);
    }
    let fixed: String = yaml_str
        .lines()
        .map(|line| {
            if let Some((key, val)) = line.split_once(": ") {
                let val = val.trim();
                if val.contains(": ") && !val.starts_with('"') && !val.starts_with('\'') {
                    format!("{key}: \"{val}\"")
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_value::<SkillFrontmatter>(yaml_to_json(&fixed)).ok()
}

fn yaml_to_json(yaml_str: &str) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let mut metadata = serde_json::Map::new();
    let mut in_metadata = false;

    for line in yaml_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed == "metadata:" {
            in_metadata = true;
            continue;
        }

        if in_metadata {
            if !line.starts_with(' ') && !line.starts_with('\t') {
                in_metadata = false;
            } else if let Some((k, v)) = trimmed.split_once(": ") {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                let val = if v == "true" {
                    serde_json::Value::Bool(true)
                } else if v == "false" {
                    serde_json::Value::Bool(false)
                } else {
                    serde_json::Value::String(v.to_string())
                };
                metadata.insert(k.trim().to_string(), val);
                continue;
            }
        }

        if !in_metadata {
            if let Some((k, v)) = trimmed.split_once(": ") {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
            }
        }
    }

    if !metadata.is_empty() {
        map.insert("metadata".to_string(), serde_json::Value::Object(metadata));
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_skill(dir: &Path, name: &str, desc: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {desc}\n---\n# {name}\n"),
        ).unwrap();
    }

    #[test]
    fn discover_finds_skills_in_standard_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        create_skill(&root.join("skills"), "my-skill", "A test skill");
        let found = discover_skills(root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "my-skill");
        assert_eq!(found[0].description, "A test skill");
    }

    #[test]
    fn discover_skips_missing_description() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill_dir = root.join("skills/bad");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: bad\n---\n").unwrap();
        let found = discover_skills(root);
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn discover_deduplicates_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        create_skill(&root.join("skills"), "dup", "First");
        create_skill(&root.join(".goose/skills"), "dup", "Second");
        let found = discover_skills(root);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn parse_frontmatter_with_colons() {
        let content = "---\nname: my-skill\ndescription: Use when: user asks about PDFs\n---\n";
        let fm = extract_frontmatter(content).unwrap();
        let parsed = serde_yaml_or_fallback(&fm).unwrap();
        assert_eq!(parsed.name.unwrap(), "my-skill");
        assert!(parsed.description.unwrap().contains("PDFs"));
    }

    #[test]
    fn internal_skills_hidden_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill_dir = root.join("skills/internal-thing");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: internal-thing\ndescription: Hidden\nmetadata:\n  internal: true\n---\n",
        ).unwrap();
        let found = discover_skills(root);
        assert_eq!(found.len(), 0);
    }
}
