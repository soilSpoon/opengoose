// Skills Load — 3-scope skill hierarchy (Rig > Project > Global)
//
// Scopes:
//   Global:  ~/.opengoose/skills/{installed,learned}/
//   Project: {cwd}/.opengoose/skills/{installed,learned}/
//   Rig:     ~/.opengoose/rigs/{rig-id}/skills/learned/
//
// Loading order: Rig (most specific) → Project → Global.
// Duplicate name at more specific scope wins.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum SkillScope {
    Installed, // manually installed, no decay
    Learned,   // auto-generated, lifecycle managed
}

#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub content: String,
    pub scope: SkillScope,
}

/// Load skills from all 3 scopes. Rig > Project > Global priority.
/// rig_id: if Some, load rig-specific learned skills
/// project_dir: if Some, load project-level skills
pub fn load_skills_3_scope(
    global_dir: &Path,
    project_dir: Option<&Path>,
    rig_id: Option<&str>,
    rigs_base: &Path,
) -> Vec<LoadedSkill> {
    let mut skills = Vec::new();
    let mut seen_names = HashSet::new();

    // 1. Rig-specific (most specific) — learned only
    if let Some(rig) = rig_id {
        let rig_learned = rigs_base.join(rig).join("skills/learned");
        scan_scope(&rig_learned, SkillScope::Learned, &mut skills, &mut seen_names);
    }

    // 2. Project
    if let Some(proj) = project_dir {
        scan_scope(
            &proj.join("installed"),
            SkillScope::Installed,
            &mut skills,
            &mut seen_names,
        );
        scan_scope(
            &proj.join("learned"),
            SkillScope::Learned,
            &mut skills,
            &mut seen_names,
        );
    }

    // 3. Global (least specific)
    scan_scope(
        &global_dir.join("installed"),
        SkillScope::Installed,
        &mut skills,
        &mut seen_names,
    );
    scan_scope(
        &global_dir.join("learned"),
        SkillScope::Learned,
        &mut skills,
        &mut seen_names,
    );

    skills
}

fn scan_scope(
    dir: &Path,
    scope: SkillScope,
    skills: &mut Vec<LoadedSkill>,
    seen: &mut HashSet<String>,
) {
    if !dir.is_dir() {
        return;
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
        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&skill_md) {
            if let Some((name, desc)) = opengoose_rig::middleware::parse_skill_header(&content) {
                if seen.insert(name.clone()) {
                    skills.push(LoadedSkill {
                        name,
                        description: desc,
                        path,
                        content,
                        scope: scope.clone(),
                    });
                }
            }
        }
    }
}

/// Convenience: load from default global path only (backward compat).
pub fn load_skills() -> Vec<LoadedSkill> {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let global_dir = home.join(".opengoose/skills");
    let rigs_base = home.join(".opengoose/rigs");
    load_skills_3_scope(&global_dir, None, None, &rigs_base)
}

/// Convenience: load from a single directory (backward compat).
/// Treats the directory as a flat scope (installed).
pub fn load_skills_from(skills_dir: &Path) -> Vec<LoadedSkill> {
    let mut skills = Vec::new();
    let mut seen = HashSet::new();
    // Scan the directory itself (not installed/learned subdirs)
    scan_scope(skills_dir, SkillScope::Installed, &mut skills, &mut seen);
    skills
}

/// Build catalog string for system prompt injection.
/// Max `cap` skills, installed first, name+description only.
pub fn build_catalog_capped(skills: &[LoadedSkill], cap: usize) -> String {
    if skills.is_empty() {
        return String::new();
    }

    // Sort: Installed first, then Learned
    let mut sorted: Vec<&LoadedSkill> = skills.iter().collect();
    sorted.sort_by_key(|s| match s.scope {
        SkillScope::Installed => 0,
        SkillScope::Learned => 1,
    });

    let mut catalog = String::from("# Available Skills\n\n");
    for skill in sorted.iter().take(cap) {
        catalog.push_str(&format!("- **{}**: {}\n", skill.name, skill.description));
    }
    catalog
}

/// Build full catalog with body excerpts (original behavior).
pub fn build_catalog(skills: &[LoadedSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut catalog = String::from(
        "# Available Skills (auto-generated from past experience)\n\
         \n\
         These skills were learned from previous tasks. Reference them when relevant.\n\n",
    );

    for skill in skills {
        catalog.push_str(&format!("## {}\n", skill.name));
        catalog.push_str(&format!("{}\n\n", skill.description));

        if let Some(body) = extract_body(&skill.content) {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                let summary = if trimmed.len() > 500 {
                    &trimmed[..500]
                } else {
                    trimmed
                };
                catalog.push_str(summary);
                catalog.push_str("\n\n");
            }
        }
    }

    catalog
}

fn extract_body(content: &str) -> Option<&str> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return Some(content);
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    Some(&rest[end + 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_skills_3_scope_test() {
        let tmp = tempfile::tempdir().unwrap();

        // Global installed
        let global = tmp.path().join("global/installed/skill-a");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: skill-a\ndescription: Global skill\n---\n",
        )
        .unwrap();

        // Rig learned
        let rig = tmp.path().join("rigs/worker-1/skills/learned/skill-b");
        std::fs::create_dir_all(&rig).unwrap();
        std::fs::write(
            rig.join("SKILL.md"),
            "---\nname: skill-b\ndescription: Use when testing\n---\n",
        )
        .unwrap();

        let skills = load_skills_3_scope(
            &tmp.path().join("global"),
            None,
            Some("worker-1"),
            &tmp.path().join("rigs"),
        );
        assert_eq!(skills.len(), 2);
        // Rig skill first (more specific)
        assert_eq!(skills[0].name, "skill-b");
        assert_eq!(skills[0].scope, SkillScope::Learned);
        assert_eq!(skills[1].name, "skill-a");
        assert_eq!(skills[1].scope, SkillScope::Installed);
    }

    #[test]
    fn rig_scope_overrides_global() {
        let tmp = tempfile::tempdir().unwrap();

        // Global
        let global = tmp.path().join("global/installed/same-name");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: same-name\ndescription: Global version\n---\n",
        )
        .unwrap();

        // Rig (same name)
        let rig = tmp.path().join("rigs/w1/skills/learned/same-name");
        std::fs::create_dir_all(&rig).unwrap();
        std::fs::write(
            rig.join("SKILL.md"),
            "---\nname: same-name\ndescription: Rig version\n---\n",
        )
        .unwrap();

        let skills = load_skills_3_scope(
            &tmp.path().join("global"),
            None,
            Some("w1"),
            &tmp.path().join("rigs"),
        );
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Rig version");
    }

    #[test]
    fn project_scope_overrides_global() {
        let tmp = tempfile::tempdir().unwrap();

        // Global installed
        let global = tmp.path().join("global/installed/shared");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: shared\ndescription: Global version\n---\n",
        )
        .unwrap();

        // Project installed (same name)
        let project = tmp.path().join("project/installed/shared");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("SKILL.md"),
            "---\nname: shared\ndescription: Project version\n---\n",
        )
        .unwrap();

        let skills = load_skills_3_scope(
            &tmp.path().join("global"),
            Some(&tmp.path().join("project")),
            None,
            &tmp.path().join("rigs"),
        );
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Project version");
    }

    #[test]
    fn catalog_cap_limits_output() {
        let skills: Vec<LoadedSkill> = (0..15)
            .map(|i| LoadedSkill {
                name: format!("skill-{i}"),
                description: format!("Description {i}"),
                path: PathBuf::from(format!("/tmp/skill-{i}")),
                content: String::new(),
                scope: SkillScope::Learned,
            })
            .collect();
        let catalog = build_catalog_capped(&skills, 10);
        assert_eq!(catalog.matches("- **").count(), 10);
    }

    #[test]
    fn catalog_installed_before_learned() {
        let skills = vec![
            LoadedSkill {
                name: "learned-1".into(),
                description: "L".into(),
                path: PathBuf::new(),
                content: String::new(),
                scope: SkillScope::Learned,
            },
            LoadedSkill {
                name: "installed-1".into(),
                description: "I".into(),
                path: PathBuf::new(),
                content: String::new(),
                scope: SkillScope::Installed,
            },
        ];
        let catalog = build_catalog_capped(&skills, 10);
        let installed_pos = catalog.find("installed-1").unwrap();
        let learned_pos = catalog.find("learned-1").unwrap();
        assert!(installed_pos < learned_pos);
    }

    #[test]
    fn load_skills_from_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = load_skills_from(tmp.path());
        assert!(skills.is_empty());
    }

    #[test]
    fn load_skills_from_populated_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Do the thing\n---\n\nDetails here.\n",
        )
        .unwrap();

        let skills = load_skills_from(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
    }

    #[test]
    fn build_catalog_formats_skills() {
        let skills = vec![LoadedSkill {
            name: "always-test".into(),
            description: "Always write tests".into(),
            path: PathBuf::from("/tmp/always-test"),
            content: "---\nname: always-test\ndescription: Always write tests\n---\n\nWrite unit tests for every function.\n".into(),
            scope: SkillScope::Learned,
        }];
        let catalog = build_catalog(&skills);
        assert!(catalog.contains("always-test"));
        assert!(catalog.contains("Write unit tests"));
    }

    #[test]
    fn empty_skills_returns_empty_catalog() {
        assert_eq!(build_catalog_capped(&[], 10), String::new());
        assert_eq!(build_catalog(&[]), String::new());
    }
}
