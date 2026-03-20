// Skills Load — thin re-export layer + backward-compat wrapper.
//
// All logic lives in opengoose-skills::{loader, lifecycle, catalog, metadata}.

pub use opengoose_skills::lifecycle::{determine_lifecycle, Lifecycle};
pub use opengoose_skills::loader::{
    extract_body, load_dormant_and_archived, load_skills, update_inclusion_tracking, LoadedSkill,
    SkillScope,
};
pub use opengoose_skills::metadata::{is_effective, read_metadata};

/// Backward-compat wrapper: load skills using home_dir as base_dir.
/// Callers in the binary crate pass (rig_id, project_dir) without a base_dir.
pub fn load_skills_for(rig_id: Option<&str>, project_dir: Option<&std::path::Path>) -> Vec<LoadedSkill> {
    let base_dir = crate::home_dir();
    load_skills(&base_dir, rig_id, project_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use opengoose_skills::metadata::{Effectiveness, GeneratedFrom, SkillMetadata};
    use std::path::PathBuf;

    /// Build catalog string for system prompt injection (test-only).
    fn build_catalog_capped(skills: &[LoadedSkill], cap: usize) -> String {
        if skills.is_empty() {
            return String::new();
        }

        let mut sorted: Vec<&LoadedSkill> = skills
            .iter()
            .filter(|s| {
                if s.scope == SkillScope::Installed {
                    return true;
                }
                if let Some(meta) = read_metadata(&s.path) {
                    let active =
                        determine_lifecycle(&meta.generated_at, meta.last_included_at.as_deref())
                            == Lifecycle::Active;
                    let ineffective = is_effective(&meta) == Some(false);
                    active && !ineffective
                } else {
                    true
                }
            })
            .collect();

        if sorted.is_empty() {
            return String::new();
        }

        sorted.sort_by_key(|s| match s.scope {
            SkillScope::Installed => (0, 0),
            SkillScope::Learned => {
                let rank = read_metadata(&s.path)
                    .and_then(|meta| is_effective(&meta))
                    .map(|eff| if eff { 1 } else { 3 })
                    .unwrap_or(2);
                (1, rank)
            }
        });

        let mut catalog = String::from("# Available Skills\n\n");
        for skill in sorted.iter().take(cap) {
            catalog.push_str(&format!("- **{}**: {}\n", skill.name, skill.description));

            if skill.scope == SkillScope::Learned {
                update_inclusion_tracking(&skill.path);
            }
        }
        catalog
    }

    #[test]
    fn load_skills_for_loads_all_scopes() {
        let tmp = tempfile::tempdir().unwrap();

        // Global installed: base/.opengoose/skills/installed/skill-a
        let global = tmp.path().join(".opengoose/skills/installed/skill-a");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: skill-a\ndescription: Global skill\n---\n",
        )
        .unwrap();

        // Rig learned: base/.opengoose/rigs/worker-1/skills/learned/skill-b
        let rig = tmp.path().join(".opengoose/rigs/worker-1/skills/learned/skill-b");
        std::fs::create_dir_all(&rig).unwrap();
        std::fs::write(
            rig.join("SKILL.md"),
            "---\nname: skill-b\ndescription: Use when testing\n---\n",
        )
        .unwrap();

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
        let tmp = tempfile::tempdir().unwrap();

        // Global
        let global = tmp.path().join(".opengoose/skills/installed/same-name");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: same-name\ndescription: Global version\n---\n",
        )
        .unwrap();

        // Rig (same name)
        let rig = tmp.path().join(".opengoose/rigs/w1/skills/learned/same-name");
        std::fs::create_dir_all(&rig).unwrap();
        std::fs::write(
            rig.join("SKILL.md"),
            "---\nname: same-name\ndescription: Rig version\n---\n",
        )
        .unwrap();

        let skills = load_skills(tmp.path(), Some("w1"), None);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Rig version");
    }

    #[test]
    fn project_scope_overrides_global() {
        let tmp = tempfile::tempdir().unwrap();

        // Global installed
        let global = tmp.path().join(".opengoose/skills/installed/shared");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(
            global.join("SKILL.md"),
            "---\nname: shared\ndescription: Global version\n---\n",
        )
        .unwrap();

        // Project installed (same name) — project_dir is a skills dir with installed/
        let project_dir = tmp.path().join("project");
        let project = project_dir.join("installed/shared");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("SKILL.md"),
            "---\nname: shared\ndescription: Project version\n---\n",
        )
        .unwrap();

        let skills = load_skills(tmp.path(), None, Some(&project_dir));
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
    fn empty_skills_returns_empty_catalog() {
        assert_eq!(build_catalog_capped(&[], 10), String::new());
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
        let tmp = tempfile::tempdir().unwrap();

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
        assert!(
            catalog.is_empty(),
            "dormant learned skill should be excluded"
        );
    }

    // -----------------------------------------------------------------------
    // is_effective tests
    // -----------------------------------------------------------------------

    #[test]
    fn is_effective_not_enough_data() {
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

    fn write_test_metadata(dir: &std::path::Path, date: &str) {
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
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn load_dormant_and_archived_filters_active() {
        let tmp = tempfile::tempdir().unwrap();

        // Active skill (recent)
        let active_dir = tmp.path().join(".opengoose/rigs/r1/skills/learned/active-skill");
        std::fs::create_dir_all(&active_dir).unwrap();
        std::fs::write(
            active_dir.join("SKILL.md"),
            "---\nname: active-skill\ndescription: Use when active\n---\n",
        )
        .unwrap();
        let now = Utc::now().to_rfc3339();
        write_test_metadata(&active_dir, &now);

        // Dormant skill (60 days old)
        let dormant_dir = tmp.path().join(".opengoose/rigs/r1/skills/learned/dormant-skill");
        std::fs::create_dir_all(&dormant_dir).unwrap();
        std::fs::write(
            dormant_dir.join("SKILL.md"),
            "---\nname: dormant-skill\ndescription: Use when dormant\n---\n",
        )
        .unwrap();
        let old = (Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        write_test_metadata(&dormant_dir, &old);

        let result = load_dormant_and_archived(
            &tmp.path().join(".opengoose/skills"),
            None,
            &tmp.path().join(".opengoose/rigs"),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "dormant-skill");
    }

    #[test]
    fn inclusion_tracking_increments_count() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("tracked-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

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
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        update_inclusion_tracking(&skill_dir);
        update_inclusion_tracking(&skill_dir);

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated.effectiveness.injected_count, 2);
        assert!(updated.last_included_at.is_some());
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

    #[test]
    fn catalog_excludes_ineffective_learned() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("bad-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: Some(Utc::now().to_rfc3339()),
            skill_version: 1,
            effectiveness: Effectiveness {
                injected_count: 5,
                subsequent_scores: vec![0.2, 0.3, 0.25],
            },
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let skills = vec![LoadedSkill {
            name: "bad-skill".into(),
            description: "Use when bad".into(),
            path: skill_dir,
            content: String::new(),
            scope: SkillScope::Learned,
        }];

        let catalog = build_catalog_capped(&skills, 10);
        assert!(
            catalog.is_empty(),
            "ineffective learned skill should be excluded"
        );
    }

    #[test]
    fn extract_body_with_no_frontmatter_returns_content() {
        let content = "# Just a header\nNo frontmatter here.\n";
        let body = extract_body(content).unwrap();
        assert!(body.contains("Just a header"));
    }

    #[test]
    fn extract_body_with_frontmatter_but_no_closing_returns_none() {
        let content = "---\nname: skill\n";
        assert!(extract_body(content).is_none());
    }

    #[test]
    fn update_inclusion_tracking_skips_invalid_metadata_json() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("bad-meta");
        std::fs::create_dir_all(&skill_dir).unwrap();
        // Valid JSON but not a SkillMetadata → serde_json::from_str fails → silent skip
        std::fs::write(skill_dir.join("metadata.json"), r#"{"not": "a skill metadata"}"#).unwrap();
        // Should not panic
        update_inclusion_tracking(&skill_dir);
    }

    #[test]
    fn load_dormant_and_archived_with_project_dir() {
        let tmp = tempfile::tempdir().unwrap();

        // Project dormant skill
        let proj_skill = tmp.path().join("project/learned/proj-dormant");
        std::fs::create_dir_all(&proj_skill).unwrap();
        std::fs::write(
            proj_skill.join("SKILL.md"),
            "---\nname: proj-dormant\ndescription: Use when dormant\n---\n",
        )
        .unwrap();
        let old = (Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        let meta = serde_json::json!({
            "generated_from": {"stamp_id": 1, "work_item_id": 1, "dimension": "Q", "score": 0.2},
            "generated_at": old,
            "evolver_work_item_id": null,
            "last_included_at": old,
            "effectiveness": {"injected_count": 0, "subsequent_scores": []},
            "skill_version": 1
        });
        std::fs::write(
            proj_skill.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let result = load_dormant_and_archived(
            &tmp.path().join(".opengoose/skills"),
            Some(&tmp.path().join("project")),
            &tmp.path().join(".opengoose/rigs"),
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "proj-dormant");
    }

    #[test]
    fn load_dormant_and_archived_excludes_skill_without_metadata() {
        let tmp = tempfile::tempdir().unwrap();

        // Learned skill with SKILL.md but NO metadata.json
        let skill_dir = tmp.path().join(".opengoose/rigs/r1/skills/learned/no-meta-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: no-meta-skill\ndescription: Use when testing\n---\n",
        )
        .unwrap();
        // No metadata.json written

        let result = load_dormant_and_archived(
            &tmp.path().join(".opengoose/skills"),
            None,
            &tmp.path().join(".opengoose/rigs"),
        );
        // Skill without metadata is excluded (read_metadata returns None → false)
        assert!(result.is_empty());
    }

    #[test]
    fn catalog_sorts_effective_before_unknown() {
        let tmp = tempfile::tempdir().unwrap();

        let make_skill = |name: &str, scores: Vec<f32>| -> LoadedSkill {
            let dir = tmp.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: Use when {name}\n---\n"),
            )
            .unwrap();
            let meta = SkillMetadata {
                generated_from: GeneratedFrom {
                    stamp_id: 1,
                    work_item_id: 1,
                    dimension: "Quality".into(),
                    score: 0.2,
                },
                generated_at: Utc::now().to_rfc3339(),
                evolver_work_item_id: None,
                last_included_at: Some(Utc::now().to_rfc3339()),
                skill_version: 1,
                effectiveness: Effectiveness {
                    injected_count: 5,
                    subsequent_scores: scores,
                },
            };
            std::fs::write(
                dir.join("metadata.json"),
                serde_json::to_string_pretty(&meta).unwrap(),
            )
            .unwrap();
            LoadedSkill {
                name: name.into(),
                description: format!("Use when {name}"),
                path: dir,
                content: String::new(),
                scope: SkillScope::Learned,
            }
        };

        let skills = vec![
            make_skill("unknown-skill", vec![0.5]), // < 3 scores = unknown
            make_skill("effective-skill", vec![0.5, 0.6, 0.7]), // avg 0.6, improvement 0.4 >= 0.2 = effective
        ];

        let catalog = build_catalog_capped(&skills, 10);
        let eff_pos = catalog.find("effective-skill").unwrap();
        let unk_pos = catalog.find("unknown-skill").unwrap();
        assert!(eff_pos < unk_pos, "effective should come before unknown");
    }
}
