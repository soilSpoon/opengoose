// Skills Load — 3-scope skill hierarchy (Rig > Project > Global)
//
// Scopes:
//   Global:  ~/.opengoose/skills/{installed,learned}/
//   Project: {cwd}/.opengoose/skills/{installed,learned}/
//   Rig:     ~/.opengoose/rigs/{rig-id}/skills/learned/
//
// Loading order: Rig (most specific) → Project → Global.
// Duplicate name at more specific scope wins.

use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::skills::evolve::SkillMetadata;

// ---------------------------------------------------------------------------
// Lifecycle — 3-stage decay for learned skills only
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Lifecycle {
    Active,   // 0-30 days since last_included_at (or generated_at)
    Dormant,  // 31-120 days
    Archived, // 121+ days
}

pub fn determine_lifecycle(generated_at: &str, last_included_at: Option<&str>) -> Lifecycle {
    let last = last_included_at
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| {
            DateTime::parse_from_rfc3339(generated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now())
        });

    let days = (Utc::now() - last).num_days();
    if days <= 30 {
        Lifecycle::Active
    } else if days <= 120 {
        Lifecycle::Dormant
    } else {
        Lifecycle::Archived
    }
}

pub fn read_metadata(skill_path: &Path) -> Option<SkillMetadata> {
    let meta_path = skill_path.join("metadata.json");
    let content = std::fs::read_to_string(meta_path).ok()?;
    serde_json::from_str(&content).ok()
}

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
/// Learned skills must be Active to be included (Dormant/Archived are skipped).
pub fn build_catalog_capped(skills: &[LoadedSkill], cap: usize) -> String {
    if skills.is_empty() {
        return String::new();
    }

    // Filter: all installed + only active learned
    let mut sorted: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| {
            if s.scope == SkillScope::Installed {
                return true;
            }
            // For learned: check lifecycle
            if let Some(meta) = read_metadata(&s.path) {
                determine_lifecycle(&meta.generated_at, meta.last_included_at.as_deref())
                    == Lifecycle::Active
            } else {
                true // no metadata = treat as active
            }
        })
        .collect();

    if sorted.is_empty() {
        return String::new();
    }

    // Sort: Installed first, then Learned
    sorted.sort_by_key(|s| match s.scope {
        SkillScope::Installed => 0,
        SkillScope::Learned => 1,
    });

    let mut catalog = String::from("# Available Skills\n\n");
    for skill in sorted.iter().take(cap) {
        catalog.push_str(&format!("- **{}**: {}\n", skill.name, skill.description));

        // Update last_included_at for learned skills
        if skill.scope == SkillScope::Learned {
            update_last_included_at(&skill.path);
        }
    }
    catalog
}

fn update_last_included_at(skill_path: &Path) {
    let meta_path = skill_path.join("metadata.json");
    if let Ok(content) = std::fs::read_to_string(&meta_path) {
        if let Ok(mut meta) = serde_json::from_str::<SkillMetadata>(&content) {
            meta.last_included_at = Some(Utc::now().to_rfc3339());
            if let Ok(json) = serde_json::to_string_pretty(&meta) {
                let _ = std::fs::write(&meta_path, json);
            }
        }
    }
}

/// Determine if a skill is effective based on subsequent scores.
/// Returns None if not enough data (< 3 scores).
/// Returns Some(true) if average improved by 0.2+ over generation score.
/// Returns Some(false) if no improvement.
pub fn is_effective(meta: &SkillMetadata) -> Option<bool> {
    let scores = &meta.effectiveness.subsequent_scores;
    if scores.len() < 3 {
        return None; // not enough data
    }
    let avg: f32 = scores.iter().sum::<f32>() / scores.len() as f32;
    let improvement = avg - meta.generated_from.score;
    Some(improvement >= 0.2)
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

    // -----------------------------------------------------------------------
    // Lifecycle tests
    // -----------------------------------------------------------------------

    #[test]
    fn lifecycle_active_when_recent() {
        let now = Utc::now().to_rfc3339();
        assert_eq!(determine_lifecycle(&now, Some(&now)), Lifecycle::Active);
    }

    #[test]
    fn lifecycle_dormant_after_30_days() {
        let old = (Utc::now() - chrono::Duration::days(35)).to_rfc3339();
        assert_eq!(determine_lifecycle(&old, Some(&old)), Lifecycle::Dormant);
    }

    #[test]
    fn lifecycle_archived_after_120_days() {
        let old = (Utc::now() - chrono::Duration::days(150)).to_rfc3339();
        assert_eq!(determine_lifecycle(&old, Some(&old)), Lifecycle::Archived);
    }

    #[test]
    fn lifecycle_uses_generated_at_when_no_last_included() {
        let now = Utc::now().to_rfc3339();
        assert_eq!(determine_lifecycle(&now, None), Lifecycle::Active);
    }

    #[test]
    fn lifecycle_boundary_30_days_is_active() {
        let edge = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        assert_eq!(determine_lifecycle(&edge, Some(&edge)), Lifecycle::Active);
    }

    #[test]
    fn lifecycle_boundary_120_days_is_dormant() {
        let edge = (Utc::now() - chrono::Duration::days(120)).to_rfc3339();
        assert_eq!(determine_lifecycle(&edge, Some(&edge)), Lifecycle::Dormant);
    }

    #[test]
    fn lifecycle_boundary_121_days_is_archived() {
        let edge = (Utc::now() - chrono::Duration::days(121)).to_rfc3339();
        assert_eq!(determine_lifecycle(&edge, Some(&edge)), Lifecycle::Archived);
    }

    #[test]
    fn catalog_capped_skips_dormant_learned() {
        use crate::skills::evolve::{Effectiveness, GeneratedFrom, SkillMetadata};

        let tmp = tempfile::tempdir().unwrap();

        // Create a learned skill with metadata dated 60 days ago (dormant)
        let skill_dir = tmp.path().join("dormant-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let old_date = (Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.3,
            },
            generated_at: old_date.clone(),
            evolver_work_item_id: None,
            last_included_at: Some(old_date),
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 1,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let skills = vec![LoadedSkill {
            name: "dormant-skill".into(),
            description: "Use when dormant".into(),
            path: skill_dir,
            content: String::new(),
            scope: SkillScope::Learned,
        }];

        let catalog = build_catalog_capped(&skills, 10);
        assert!(catalog.is_empty(), "dormant learned skill should be excluded");
    }

    // -----------------------------------------------------------------------
    // is_effective tests
    // -----------------------------------------------------------------------

    #[test]
    fn is_effective_not_enough_data() {
        use crate::skills::evolve::{Effectiveness, GeneratedFrom};

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![0.5],
            },
            skill_version: 1,
        };
        assert_eq!(is_effective(&meta), None);
    }

    #[test]
    fn is_effective_improved() {
        use crate::skills::evolve::{Effectiveness, GeneratedFrom};

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![0.5, 0.6, 0.7],
            },
            skill_version: 1,
        };
        assert_eq!(is_effective(&meta), Some(true)); // avg 0.6 - 0.2 = 0.4 >= 0.2
    }

    #[test]
    fn is_effective_not_improved() {
        use crate::skills::evolve::{Effectiveness, GeneratedFrom};

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![0.2, 0.3, 0.25],
            },
            skill_version: 1,
        };
        assert_eq!(is_effective(&meta), Some(false)); // avg 0.25 - 0.2 = 0.05 < 0.2
    }

    #[test]
    fn catalog_capped_includes_installed_always() {
        let skills = vec![LoadedSkill {
            name: "always-here".into(),
            description: "I".into(),
            path: PathBuf::from("/nonexistent"),
            content: String::new(),
            scope: SkillScope::Installed,
        }];
        let catalog = build_catalog_capped(&skills, 10);
        assert!(catalog.contains("always-here"));
    }
}
