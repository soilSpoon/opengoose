// catalog.rs — Skill catalog generation for system prompt injection
//
// Filters, sorts (installed first, then by effectiveness), and caps skill entries.

use crate::lifecycle::{Lifecycle, determine_lifecycle};
use crate::loader::{LoadedSkill, SkillScope};
use crate::metadata::{is_effective, read_metadata};

/// Build catalog string for system prompt injection.
/// Filters out dormant/archived learned skills and ineffective learned skills.
/// Sorts installed before learned, effective before unknown.
/// Caps output to `cap` entries.
pub fn build_catalog_capped(skills: &[LoadedSkill], cap: usize) -> String {
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
            crate::loader::update_inclusion_tracking(&skill.path);
        }
    }
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        let installed_pos = catalog
            .find("installed-1")
            .expect("catalog operation should succeed");
        let learned_pos = catalog
            .find("learned-1")
            .expect("catalog operation should succeed");
        assert!(installed_pos < learned_pos);
    }

    #[test]
    fn empty_skills_returns_empty_catalog() {
        assert_eq!(build_catalog_capped(&[], 10), String::new());
    }

    #[test]
    fn catalog_capped_skips_dormant_learned() {
        use crate::metadata::{Effectiveness, GeneratedFrom, SkillMetadata};
        use chrono::Utc;

        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");

        let skill_dir = tmp.path().join("dormant-skill");
        std::fs::create_dir_all(&skill_dir).expect("directory creation should succeed");
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
            serde_json::to_string_pretty(&meta).expect("JSON serialization should succeed"),
        )
        .expect("file write should succeed");

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
        use crate::metadata::{Effectiveness, GeneratedFrom, SkillMetadata};
        use chrono::Utc;

        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let skill_dir = tmp.path().join("bad-skill");
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
            last_included_at: Some(Utc::now().to_rfc3339()),
            skill_version: 1,
            effectiveness: Effectiveness {
                injected_count: 5,
                subsequent_scores: vec![0.2, 0.3, 0.25],
            },
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("JSON serialization should succeed"),
        )
        .expect("file write should succeed");

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
    fn catalog_sorts_effective_before_unknown() {
        use crate::metadata::{Effectiveness, GeneratedFrom, SkillMetadata};
        use chrono::Utc;

        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");

        let make_skill = |name: &str, scores: Vec<f32>| -> LoadedSkill {
            let dir = tmp.path().join(name);
            std::fs::create_dir_all(&dir).expect("directory creation should succeed");
            std::fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: Use when {name}\n---\n"),
            )
            .expect("file write should succeed");
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
                serde_json::to_string_pretty(&meta).expect("JSON serialization should succeed"),
            )
            .expect("file write should succeed");
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
        let eff_pos = catalog
            .find("effective-skill")
            .expect("catalog operation should succeed");
        let unk_pos = catalog
            .find("unknown-skill")
            .expect("catalog operation should succeed");
        assert!(eff_pos < unk_pos, "effective should come before unknown");
    }
}
