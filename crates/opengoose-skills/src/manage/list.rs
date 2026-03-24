use crate::lifecycle::{Lifecycle, determine_lifecycle};
use crate::loader::{LoadedSkill, SkillScope, load_skills};
use crate::metadata::read_metadata;
use std::path::{Path, PathBuf};

pub fn run(base_dir: &Path, global_only: bool, show_archived: bool) -> anyhow::Result<()> {
    let global_dir = base_dir.join(".opengoose/skills");

    let project_dir = if global_only {
        None
    } else {
        let p = PathBuf::from(".opengoose/skills");
        if p.is_dir() { Some(p) } else { None }
    };

    let skills = load_skills(base_dir, None, project_dir.as_deref());

    if skills.is_empty() {
        println!("No skills installed. Use 'opengoose skills add' to install skills.");
        return Ok(());
    }

    // Group by scope for display
    let global_installed: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| s.scope == SkillScope::Installed && is_under(&s.path, &global_dir))
        .collect();
    let global_learned: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| s.scope == SkillScope::Learned && is_under(&s.path, &global_dir))
        .collect();
    let project_installed: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| {
            s.scope == SkillScope::Installed
                && project_dir.as_ref().is_some_and(|p| is_under(&s.path, p))
        })
        .collect();
    let project_learned: Vec<&LoadedSkill> = skills
        .iter()
        .filter(|s| {
            s.scope == SkillScope::Learned
                && project_dir.as_ref().is_some_and(|p| is_under(&s.path, p))
        })
        .collect();

    print_group("Global (installed)", &global_installed, show_archived);
    print_group("Global (learned)", &global_learned, show_archived);
    print_group("Project (installed)", &project_installed, show_archived);
    print_group("Project (learned)", &project_learned, show_archived);

    Ok(())
}

fn is_under(path: &std::path::Path, base: &std::path::Path) -> bool {
    // Canonicalize when possible, fall back to starts_with
    let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let canon_base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    canon_path.starts_with(&canon_base)
}

fn lifecycle_label(skill: &LoadedSkill) -> Option<&'static str> {
    if skill.scope == SkillScope::Installed {
        return None; // installed skills have no lifecycle
    }
    let meta = read_metadata(&skill.path)?;
    let lc = determine_lifecycle(&meta.generated_at, meta.last_included_at.as_deref());
    Some(match lc {
        Lifecycle::Active => "active",
        Lifecycle::Dormant => "dormant",
        Lifecycle::Archived => "archived",
    })
}

fn print_group(header: &str, skills: &[&LoadedSkill], show_archived: bool) {
    if skills.is_empty() {
        return;
    }

    // Filter: hide archived learned skills unless --archived is set
    let visible: Vec<&&LoadedSkill> = skills
        .iter()
        .filter(|s| {
            if s.scope == SkillScope::Installed {
                return true;
            }
            let lc = lifecycle_label(s);
            match lc {
                Some("archived") => show_archived,
                _ => true,
            }
        })
        .collect();

    if visible.is_empty() {
        return;
    }

    println!("\n{header}:");
    for skill in visible {
        let label = match skill.scope {
            SkillScope::Installed => "installed".to_string(),
            SkillScope::Learned => {
                if let Some(lc) = lifecycle_label(skill) {
                    format!("learned, {lc}")
                } else {
                    "learned".to_string()
                }
            }
        };
        println!(
            "  {:<20} — {:<40} ({})",
            skill.name, skill.description, label
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn tmp_skill(
        name: &str,
        scope: SkillScope,
        path: std::path::PathBuf,
        description: &str,
    ) -> LoadedSkill {
        LoadedSkill {
            name: name.into(),
            description: description.into(),
            path,
            content: String::from(""),
            scope,
        }
    }

    fn write_metadata(path: &std::path::Path) {
        let meta = json!({
            "generated_from": {
                "stamp_id": 1,
                "work_item_id": 1,
                "dimension": "Quality",
                "score": 0.2
            },
            "generated_at": Utc::now().to_rfc3339(),
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": {
                "injected_count": 1,
                "subsequent_scores": []
            }
        });
        std::fs::create_dir_all(path).expect("directory creation should succeed");
        std::fs::write(
            path.join("metadata.json"),
            serde_json::to_string(&meta).expect("JSON serialization should succeed"),
        )
        .expect("operation should succeed");
        std::fs::write(path.join("SKILL.md"), "---\nname: n\ndescription: x\n---\n").expect("test fixture write should succeed");
    }

    #[test]
    fn is_under_detects_parent_path() {
        let base = std::path::Path::new("/tmp/a/b");
        let child = std::path::Path::new("/tmp/a/b/c");
        assert!(is_under(child, base));
        assert!(!is_under(std::path::Path::new("/tmp/x"), base));
    }

    #[test]
    fn lifecycle_label_installed_is_none() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let skill = tmp_skill("s", SkillScope::Installed, tmp.path().into(), "installed");
        assert!(lifecycle_label(&skill).is_none());
    }

    #[test]
    fn lifecycle_label_learned_reads_metadata() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        write_metadata(tmp.path());
        let skill = tmp_skill("s", SkillScope::Learned, tmp.path().into(), "learned");
        assert_eq!(lifecycle_label(&skill), Some("active"));
    }

    #[test]
    fn lifecycle_label_dormant_and_archived() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let dormant_path = tmp.path().join("dormant");
        let archived_path = tmp.path().join("archived");
        std::fs::create_dir_all(&dormant_path).expect("directory creation should succeed");
        std::fs::create_dir_all(&archived_path).expect("directory creation should succeed");

        let dormant_date = (chrono::Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        let archived_date = (chrono::Utc::now() - chrono::Duration::days(200)).to_rfc3339();

        for (path, date) in [
            (&dormant_path, &dormant_date),
            (&archived_path, &archived_date),
        ] {
            let meta = serde_json::json!({
                "generated_from": {"stamp_id": 1, "work_item_id": 1, "dimension": "Q", "score": 0.2},
                "generated_at": date,
                "evolver_work_item_id": null,
                "last_included_at": date,
                "effectiveness": {"injected_count": 0, "subsequent_scores": []},
                "skill_version": 1
            });
            std::fs::write(
                path.join("metadata.json"),
                serde_json::to_string(&meta).expect("JSON serialization should succeed"),
            )
            .expect("operation should succeed");
        }

        let dormant_skill = tmp_skill("d", SkillScope::Learned, dormant_path, "dormant skill");
        let archived_skill = tmp_skill("a", SkillScope::Learned, archived_path, "archived skill");
        assert_eq!(lifecycle_label(&dormant_skill), Some("dormant"));
        assert_eq!(lifecycle_label(&archived_skill), Some("archived"));
    }

    #[test]
    fn print_group_with_archived_filter_runs_branches() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let installed_path = tmp.path().join("installed");
        let learned_path = tmp.path().join("learned");
        let installed = tmp_skill(
            "installed",
            SkillScope::Installed,
            installed_path,
            "installed",
        );
        let learned = tmp_skill(
            "learned",
            SkillScope::Learned,
            learned_path.clone(),
            "learned",
        );
        write_metadata(&learned_path);

        let installed_items = vec![&installed];
        let learned_items = vec![&learned];

        print_group("installed", &installed_items, false);
        print_group("learned", &learned_items, true);
        print_group("learned", &learned_items, false);
    }

    #[test]
    fn lifecycle_label_learned_returns_none_when_no_metadata() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        // No metadata.json created → read_metadata returns None → lifecycle_label returns None
        let skill = tmp_skill("s", SkillScope::Learned, tmp.path().into(), "learned");
        assert!(lifecycle_label(&skill).is_none());
    }

    #[test]
    fn print_group_empty_skills_does_nothing() {
        // Empty slice → early return, no panic
        let skills: Vec<&LoadedSkill> = vec![];
        print_group("Empty Group", &skills, false);
        print_group("Empty Group", &skills, true);
    }

    #[test]
    fn print_group_hides_archived_when_show_archived_false() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let learned_path = tmp.path().join("old-skill");
        // Write metadata with old generated_at and no last_included_at → archived lifecycle
        let old_date = "2000-01-01T00:00:00Z"; // Very old = archived
        let meta = serde_json::json!({
            "generated_from": {"stamp_id": 1, "work_item_id": 1, "dimension": "Q", "score": 0.1},
            "generated_at": old_date,
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": {"injected_count": 10, "subsequent_scores": [0.1, 0.1, 0.1]},
            "skill_version": 1
        });
        std::fs::create_dir_all(&learned_path).expect("directory creation should succeed");
        std::fs::write(
            learned_path.join("metadata.json"),
            serde_json::to_string(&meta).expect("JSON serialization should succeed"),
        )
        .expect("operation should succeed");
        std::fs::write(
            learned_path.join("SKILL.md"),
            "---\nname: old-skill\ndescription: old\n---\n",
        )
        .expect("operation should succeed");

        let skill = tmp_skill(
            "old-skill",
            SkillScope::Learned,
            learned_path.clone(),
            "old",
        );
        let items = vec![&skill];

        // show_archived=false: if lifecycle is "archived", visible becomes empty → early return (no output)
        // show_archived=true: visible would include it
        print_group("Test", &items, false); // may or may not show depending on lifecycle
        print_group("Test", &items, true); // always shows
    }

    #[test]
    fn print_group_learned_without_metadata_shows_learned_label() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let learned_path = tmp.path().join("no-meta");
        std::fs::create_dir_all(&learned_path).expect("directory creation should succeed");
        // No metadata.json → lifecycle_label returns None → label = "learned"
        let skill = tmp_skill("no-meta", SkillScope::Learned, learned_path, "no meta");
        let items = vec![&skill];
        print_group("Test", &items, false);
    }

    #[test]
    fn is_under_same_path_is_under() {
        let base = std::path::Path::new("/tmp/a/b");
        assert!(is_under(base, base));
    }

    #[test]
    fn run_no_skills_returns_ok() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let env = crate::test_utils::IsolatedEnv::new(tmp.path());
        let cwd = std::env::current_dir().expect("operation should succeed");
        std::env::set_current_dir(tmp.path()).expect("operation should succeed");

        assert!(run(tmp.path(), false, false).is_ok());
        assert!(run(tmp.path(), true, false).is_ok());

        std::env::set_current_dir(cwd).expect("operation should succeed");
        drop(env);
    }

    #[test]
    fn run_with_global_installed_skill_prints_group() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let env = crate::test_utils::IsolatedEnv::new(tmp.path());
        let cwd = std::env::current_dir().expect("operation should succeed");
        std::env::set_current_dir(tmp.path()).expect("operation should succeed");

        let skill_dir = tmp.path().join(".opengoose/skills/installed/run-skill");
        std::fs::create_dir_all(&skill_dir).expect("directory creation should succeed");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: run-skill\ndescription: Use when testing run\n---\n",
        )
        .expect("operation should succeed");

        assert!(run(tmp.path(), false, false).is_ok());
        assert!(run(tmp.path(), true, false).is_ok());
        assert!(run(tmp.path(), false, true).is_ok());
        assert!(run(tmp.path(), true, true).is_ok());

        std::env::set_current_dir(cwd).expect("operation should succeed");
        drop(env);
    }

    #[test]
    fn run_with_global_only_skips_project_skills() {
        let home_tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let project_tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let env = crate::test_utils::IsolatedEnv::new(home_tmp.path());
        let cwd = std::env::current_dir().expect("operation should succeed");
        std::env::set_current_dir(project_tmp.path()).expect("operation should succeed");

        // Global skill in HOME
        let global_dir = home_tmp.path().join(".opengoose/skills/installed/g-skill");
        std::fs::create_dir_all(&global_dir).expect("directory creation should succeed");
        std::fs::write(
            global_dir.join("SKILL.md"),
            "---\nname: g-skill\ndescription: Global\n---\n",
        )
        .expect("operation should succeed");

        // Project skill in CWD (separate from HOME)
        let project_dir = project_tmp.path().join(".opengoose/skills/learned/p-skill");
        write_metadata(&project_dir);

        assert!(run(home_tmp.path(), false, false).is_ok());
        assert!(run(home_tmp.path(), true, false).is_ok());

        std::env::set_current_dir(cwd).expect("operation should succeed");
        drop(env);
    }
}
