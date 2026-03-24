// loader.rs — 3-scope skill hierarchy (Rig > Project > Global)
//
// Scopes:
//   Global:  {base_dir}/.opengoose/skills/{installed,learned}/
//   Project: {project_dir}/.opengoose/skills/{installed,learned}/
//   Rig:     {base_dir}/.opengoose/rigs/{rig-id}/skills/learned/
//
// Loading order: Rig (most specific) → Project → Global.
// Duplicate name at more specific scope wins.

use chrono::Utc;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::lifecycle::{Lifecycle, determine_lifecycle};
use crate::metadata::{SkillMetadata, parse_frontmatter, read_metadata};

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

/// Unified public API: load skills from all 3 scopes.
/// base_dir: filesystem root (replaces home_dir)
/// rig_id: if Some, load rig-specific learned skills
/// project_dir: if Some, load project-level skills
pub fn load_skills(
    base_dir: &Path,
    rig_id: Option<&str>,
    project_dir: Option<&Path>,
) -> Vec<LoadedSkill> {
    let global_dir = base_dir.join(".opengoose/skills");
    let rigs_base = base_dir.join(".opengoose/rigs");
    load_skills_inner(&global_dir, project_dir, rig_id, &rigs_base)
}

/// Load skills from all 3 scopes. Rig > Project > Global priority.
fn load_skills_inner(
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
        scan_scope(
            &rig_learned,
            SkillScope::Learned,
            &mut skills,
            &mut seen_names,
        );
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

pub fn scan_scope(
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
        if let Ok(content) = std::fs::read_to_string(&skill_md)
            && let Some(fm) = parse_frontmatter(&content)
            && seen.insert(fm.name.clone())
        {
            skills.push(LoadedSkill {
                name: fm.name,
                description: fm.description,
                path,
                content,
                scope: scope.clone(),
            });
        }
    }
}

/// Load only Dormant and Archived learned skills across all scopes.
pub fn load_dormant_and_archived(
    global_dir: &Path,
    project_dir: Option<&Path>,
    rigs_base: &Path,
) -> Vec<LoadedSkill> {
    let mut skills = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(entries) = std::fs::read_dir(rigs_base) {
        for entry in entries.flatten() {
            let learned_dir = entry.path().join("skills/learned");
            scan_scope(&learned_dir, SkillScope::Learned, &mut skills, &mut seen);
        }
    }

    if let Some(proj) = project_dir {
        scan_scope(
            &proj.join("learned"),
            SkillScope::Learned,
            &mut skills,
            &mut seen,
        );
    }
    scan_scope(
        &global_dir.join("learned"),
        SkillScope::Learned,
        &mut skills,
        &mut seen,
    );

    skills.retain(|s| {
        if let Some(meta) = read_metadata(&s.path) {
            let lifecycle =
                determine_lifecycle(&meta.generated_at, meta.last_included_at.as_deref());
            lifecycle == Lifecycle::Dormant || lifecycle == Lifecycle::Archived
        } else {
            false
        }
    });

    skills
}

pub fn update_inclusion_tracking(skill_path: &Path) {
    let meta_path = skill_path.join("metadata.json");
    if let Ok(content) = std::fs::read_to_string(&meta_path)
        && let Ok(mut meta) = serde_json::from_str::<SkillMetadata>(&content)
    {
        meta.last_included_at = Some(Utc::now().to_rfc3339());
        meta.effectiveness.injected_count += 1;
        if let Ok(json) = serde_json::to_string_pretty(&meta) {
            let _ = std::fs::write(&meta_path, json);
        }
    }
}

pub fn extract_body(content: &str) -> Option<&str> {
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
    fn load_skills_loads_all_scopes() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");

        // Global installed
        let global = tmp.path().join(".opengoose/skills/installed/skill-a");
        std::fs::create_dir_all(&global).expect("directory creation should succeed");
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: skill-a\ndescription: Global skill\n---\n",
        )
        .expect("operation should succeed");

        // Rig learned
        let rig = tmp
            .path()
            .join(".opengoose/rigs/worker-1/skills/learned/skill-b");
        std::fs::create_dir_all(&rig).expect("directory creation should succeed");
        std::fs::write(
            rig.join("SKILL.md"),
            "---\nname: skill-b\ndescription: Use when testing\n---\n",
        )
        .expect("operation should succeed");

        let skills = load_skills(tmp.path(), Some("worker-1"), None);
        assert_eq!(skills.len(), 2);
        // Rig skill first (more specific)
        assert_eq!(skills[0].name, "skill-b");
        assert_eq!(skills[0].scope, SkillScope::Learned);
        assert_eq!(skills[1].name, "skill-a");
        assert_eq!(skills[1].scope, SkillScope::Installed);
    }

    #[test]
    fn rig_scope_overrides_global() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");

        // Global
        let global = tmp.path().join(".opengoose/skills/installed/same-name");
        std::fs::create_dir_all(&global).expect("directory creation should succeed");
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: same-name\ndescription: Global version\n---\n",
        )
        .expect("operation should succeed");

        // Rig (same name)
        let rig = tmp
            .path()
            .join(".opengoose/rigs/w1/skills/learned/same-name");
        std::fs::create_dir_all(&rig).expect("directory creation should succeed");
        std::fs::write(
            rig.join("SKILL.md"),
            "---\nname: same-name\ndescription: Rig version\n---\n",
        )
        .expect("operation should succeed");

        let skills = load_skills(tmp.path(), Some("w1"), None);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Rig version");
    }

    #[test]
    fn project_scope_overrides_global() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");

        // Global installed
        let global = tmp.path().join(".opengoose/skills/installed/shared");
        std::fs::create_dir_all(&global).expect("directory creation should succeed");
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: shared\ndescription: Global version\n---\n",
        )
        .expect("operation should succeed");

        // Project installed (same name)
        let project = tmp.path().join("project/installed/shared");
        std::fs::create_dir_all(&project).expect("directory creation should succeed");
        std::fs::write(
            project.join("SKILL.md"),
            "---\nname: shared\ndescription: Project version\n---\n",
        )
        .expect("operation should succeed");

        let skills = load_skills(tmp.path(), None, Some(&tmp.path().join("project")));
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Project version");
    }

    fn write_test_metadata(dir: &Path, date: &str) {
        use crate::metadata::{Effectiveness, GeneratedFrom};
        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: date.to_string(),
            evolver_work_item_id: None,
            last_included_at: Some(date.to_string()),
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 1,
        };
        std::fs::write(
            dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("operation should succeed"),
        )
        .expect("operation should succeed");
    }

    #[test]
    fn load_dormant_and_archived_filters_active() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");

        // Active skill (recent)
        let active_dir = tmp.path().join("rigs/r1/skills/learned/active-skill");
        std::fs::create_dir_all(&active_dir).expect("directory creation should succeed");
        std::fs::write(
            active_dir.join("SKILL.md"),
            "---\nname: active-skill\ndescription: Use when active\n---\n",
        )
        .expect("operation should succeed");
        let now = Utc::now().to_rfc3339();
        write_test_metadata(&active_dir, &now);

        // Dormant skill (60 days old)
        let dormant_dir = tmp.path().join("rigs/r1/skills/learned/dormant-skill");
        std::fs::create_dir_all(&dormant_dir).expect("directory creation should succeed");
        std::fs::write(
            dormant_dir.join("SKILL.md"),
            "---\nname: dormant-skill\ndescription: Use when dormant\n---\n",
        )
        .expect("operation should succeed");
        let old = (Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        write_test_metadata(&dormant_dir, &old);

        let result =
            load_dormant_and_archived(&tmp.path().join("global"), None, &tmp.path().join("rigs"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "dormant-skill");
    }

    #[test]
    fn inclusion_tracking_increments_count() {
        use crate::metadata::{Effectiveness, GeneratedFrom};

        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let skill_dir = tmp.path().join("tracked-skill");
        std::fs::create_dir_all(&skill_dir).expect("directory creation should succeed");

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
            skill_version: 1,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![],
            },
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("operation should succeed"),
        )
        .expect("operation should succeed");

        update_inclusion_tracking(&skill_dir);
        update_inclusion_tracking(&skill_dir);

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("test file read should succeed"),
        )
        .expect("operation should succeed");
        assert_eq!(updated.effectiveness.injected_count, 2);
        assert!(updated.last_included_at.is_some());
    }
}
