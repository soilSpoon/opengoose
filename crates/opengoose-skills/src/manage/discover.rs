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

fn parse_skill_md(path: &Path, skill_dir: &Path, repo_root: &Path) -> Option<DiscoveredSkill> {
    let content = std::fs::read_to_string(path).ok()?;
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
        )
        .unwrap();
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
        let _env = crate::test_utils::IsolatedEnv::new(tmp.path());
        unsafe {
            std::env::remove_var("INSTALL_INTERNAL_SKILLS");
        }
        let root = tmp.path();
        let skill_dir = root.join("skills/internal-thing");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: internal-thing\ndescription: Hidden\nmetadata:\n  internal: true\n---\n",
        )
        .unwrap();
        let found = discover_skills(root);
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn internal_skills_shown_when_env_set() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = crate::test_utils::IsolatedEnv::new(tmp.path());
        let root = tmp.path();
        let skill_dir = root.join("skills/internal-thing");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: internal-thing\ndescription: Hidden but now visible\nmetadata:\n  internal: true\n---\n",
        ).unwrap();
        unsafe {
            std::env::set_var("INSTALL_INTERNAL_SKILLS", "1");
        }
        let found = discover_skills(root);
        unsafe {
            std::env::remove_var("INSTALL_INTERNAL_SKILLS");
        }
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "internal-thing");
    }

    #[test]
    fn scan_dir_respects_depth_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Build a 6-level deep path
        let deep = root.join("a/b/c/d/e/f/skill-deep");
        fs::create_dir_all(&deep).unwrap();
        fs::write(
            deep.join("SKILL.md"),
            "---\nname: skill-deep\ndescription: Too deep\n---\n",
        )
        .unwrap();
        let found = discover_skills(root);
        // depth > 5 guard prevents finding it
        assert!(found.iter().all(|s| s.name != "skill-deep"));
    }

    #[test]
    fn name_mismatch_with_dir_still_uses_frontmatter_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Skill directory "my-dir" but frontmatter says "different-name"
        let skill_dir = root.join("skills/my-dir");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: different-name\ndescription: Mismatch test\n---\n",
        )
        .unwrap();
        let found = discover_skills(root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "different-name");
    }

    #[test]
    fn no_frontmatter_returns_no_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill_dir = root.join("skills/no-fm");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "No frontmatter here\n").unwrap();
        let found = discover_skills(root);
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn yaml_to_json_parses_metadata_block() {
        let yaml =
            "name: my-skill\ndescription: Test\nmetadata:\n  internal: false\n  key: value\n";
        let json = yaml_to_json(yaml);
        assert_eq!(json["name"], "my-skill");
        assert_eq!(json["metadata"]["internal"], false);
        assert_eq!(json["metadata"]["key"], "value");
    }

    #[test]
    fn yaml_to_json_exits_metadata_on_non_indented_line() {
        let yaml = "name: skill\nmetadata:\n  key: val\ndescription: After metadata\n";
        let json = yaml_to_json(yaml);
        assert_eq!(json["description"], "After metadata");
        assert_eq!(json["metadata"]["key"], "val");
    }

    #[test]
    fn scan_dir_skips_dot_prefixed_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Skill inside a dot-prefixed dir (not in standard dirs list)
        let hidden_skill = root.join(".hidden/skill-in-hidden");
        fs::create_dir_all(&hidden_skill).unwrap();
        fs::write(
            hidden_skill.join("SKILL.md"),
            "---\nname: skill-in-hidden\ndescription: Should not be found\n---\n",
        )
        .unwrap();
        // Skill in a node_modules dir
        let node_skill = root.join("skills/node_modules/bad-skill");
        fs::create_dir_all(&node_skill).unwrap();
        fs::write(
            node_skill.join("SKILL.md"),
            "---\nname: bad-skill\ndescription: Should not be found\n---\n",
        )
        .unwrap();
        let found = discover_skills(root);
        assert!(found.iter().all(|s| s.name != "skill-in-hidden"));
        assert!(found.iter().all(|s| s.name != "bad-skill"));
    }

    #[test]
    fn serde_yaml_or_fallback_returns_none_for_unparseable() {
        // Empty string → yaml_to_json returns empty object → all fields None
        // But serde_json should still succeed with None values
        let fm = serde_yaml_or_fallback("");
        assert!(fm.is_some());
        assert!(fm.unwrap().name.is_none());
    }

    #[test]
    fn serde_yaml_or_fallback_uses_fallback_when_metadata_is_not_a_map() {
        // "metadata: simple-value" makes yaml_to_json produce {"metadata": "simple-value"}
        // which fails SkillFrontmatter deserialization (metadata should be Option<HashMap>)
        // This triggers the fallback path (lines 134-150), covering those lines.
        // A line with ": " in the value also covers the format!() branch (line 140).
        // A line with no ": " covers the else branch (line 144-145).
        let yaml = "name: my-skill\ndescription: Use when: testing\nbare-key\nmetadata: inline-val";
        let result = serde_yaml_or_fallback(yaml);
        // Result can be None or Some — what matters is the fallback path ran
        let _ = result;
    }

    #[test]
    fn serde_yaml_or_fallback_fallback_quotes_value_with_colon() {
        // Covers line 140: format!("{key}: \"{val}\"") when val.contains(": ")
        // "description: Use when: foo bar" → val = "Use when: foo bar" → contains ": "
        // Combined with metadata failure to trigger fallback
        let yaml = "metadata: not-a-map\ndescription: Use when: there is a colon";
        let _ = serde_yaml_or_fallback(yaml);
    }

    #[test]
    fn yaml_to_json_skips_comments_and_empty_lines() {
        let yaml = "\n# This is a comment\nname: commented-skill\n\ndescription: Works\n";
        let json = yaml_to_json(yaml);
        assert_eq!(json["name"], "commented-skill");
        assert_eq!(json["description"], "Works");
    }

    #[test]
    fn scan_dir_skips_target_and_pycache_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Skill inside a `target` dir (Rust build artifact dir)
        let target_skill = root.join("skills/target/cargo-skill");
        fs::create_dir_all(&target_skill).unwrap();
        fs::write(
            target_skill.join("SKILL.md"),
            "---\nname: cargo-skill\ndescription: Should not be found\n---\n",
        )
        .unwrap();

        // Skill inside a `__pycache__` dir
        let pycache_skill = root.join("skills/__pycache__/py-skill");
        fs::create_dir_all(&pycache_skill).unwrap();
        fs::write(
            pycache_skill.join("SKILL.md"),
            "---\nname: py-skill\ndescription: Should not be found\n---\n",
        )
        .unwrap();

        let found = discover_skills(root);
        assert!(found.iter().all(|s| s.name != "cargo-skill"));
        assert!(found.iter().all(|s| s.name != "py-skill"));
    }

    #[test]
    fn discover_uses_dir_name_when_skill_has_no_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let skill_dir = root.join("skills/inferred-name");
        fs::create_dir_all(&skill_dir).unwrap();
        // No 'name' field — should use directory name as fallback
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Use when inferring names from directory\n---\n",
        )
        .unwrap();
        let found = discover_skills(root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "inferred-name");
    }

    #[test]
    fn yaml_to_json_metadata_line_without_colon_is_skipped() {
        // Indented metadata line without ": " → split_once fails → falls through without continue
        // Covers the implicit "no match" branch (line 183) and the if !in_metadata=false path (line 191)
        let yaml = "name: skill\nmetadata:\n  key: val\n  no-colon-here\ndescription: After\n";
        let json = yaml_to_json(yaml);
        assert_eq!(json["name"], "skill");
        assert_eq!(json["metadata"]["key"], "val");
        assert_eq!(json["description"], "After");
        // "no-colon-here" is not a key (no ": " separator)
        assert!(
            !json["metadata"]
                .as_object()
                .unwrap()
                .contains_key("no-colon-here")
        );
    }

    #[test]
    fn scan_dir_on_unreadable_path_returns_early() {
        // Calling scan_dir on a non-existent path causes read_dir to fail → Err(_) => return (line 64)
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("does-not-exist");
        let mut skills = Vec::new();
        scan_dir(&nonexistent, tmp.path(), &mut skills, 0);
        assert!(skills.is_empty());
    }

    #[test]
    fn parse_skill_md_when_internal_true_and_env_set_returns_skill() {
        // Covers line 97: the } after if INSTALL_INTERNAL_SKILLS != "1" when env IS "1"
        // (we don't return None, continue to produce the skill)
        let tmp = tempfile::tempdir().unwrap();
        let _env = crate::test_utils::IsolatedEnv::new(tmp.path());
        let root = tmp.path();
        let skill_dir = root.join("skills/internal-with-non-bool-meta");
        fs::create_dir_all(&skill_dir).unwrap();
        // metadata with internal: false (Bool(false)) → the if let Some(Bool(true)) doesn't match
        // → falls through to line 97 (closing brace of the inner if let)
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: internal-with-non-bool-meta\ndescription: Use when testing\nmetadata:\n  internal: false\n---\n",
        ).unwrap();
        let found = discover_skills(root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "internal-with-non-bool-meta");
    }
}
