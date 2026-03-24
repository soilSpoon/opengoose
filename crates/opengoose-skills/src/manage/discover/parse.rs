use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use super::DiscoveredSkill;

#[derive(Deserialize)]
pub(super) struct SkillFrontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

pub(super) fn parse_skill_md(
    path: &Path,
    skill_dir: &Path,
    repo_root: &Path,
) -> Option<DiscoveredSkill> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(path = %path.display(), "failed to read SKILL.md: {e}");
            return None;
        }
    };
    let frontmatter = extract_frontmatter(&content)?;
    let fm: SkillFrontmatter = serde_yaml_or_fallback(&frontmatter)?;

    let description = fm.description.filter(|d| !d.is_empty())?;

    if let Some(meta) = &fm.metadata
        && let Some(serde_json::Value::Bool(true)) = meta.get("internal")
        && std::env::var("INSTALL_INTERNAL_SKILLS").unwrap_or_default() != "1"
    {
        return None;
    }

    let dir_name = skill_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string());
    let name = fm
        .name
        .unwrap_or_else(|| dir_name.clone().unwrap_or_else(|| "unnamed".to_string()));

    if let Some(ref dn) = dir_name
        && *dn != name
        && !dn.is_empty()
    {
        eprintln!(
            "warning: skill name '{}' doesn't match directory '{}'",
            name, dn
        );
    }

    let rel_path = skill_dir
        .strip_prefix(repo_root)
        .unwrap_or(skill_dir)
        .to_string_lossy()
        .to_string();

    Some(DiscoveredSkill {
        name,
        description,
        rel_path,
        abs_path: skill_dir.to_path_buf(),
    })
}

pub(super) fn extract_frontmatter(content: &str) -> Option<String> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    Some(rest[..end].to_string())
}

pub(super) fn serde_yaml_or_fallback(yaml_str: &str) -> Option<SkillFrontmatter> {
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
    match serde_json::from_value::<SkillFrontmatter>(yaml_to_json(&fixed)) {
        Ok(fm) => Some(fm),
        Err(e) => {
            tracing::debug!("YAML fallback parse failed: {e}");
            None
        }
    }
}

pub(super) fn yaml_to_json(yaml_str: &str) -> serde_json::Value {
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

        if !in_metadata && let Some((k, v)) = trimmed.split_once(": ") {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
    }

    if !metadata.is_empty() {
        map.insert("metadata".to_string(), serde_json::Value::Object(metadata));
    }
    serde_json::Value::Object(map)
}
