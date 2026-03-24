// Skill Evolution — thin re-export layer.
// All logic lives in opengoose-skills::evolution.
// read_conversation_log() stays here because it depends on opengoose_rig.

pub use opengoose_skills::evolution::parser::{
    EvolveAction, SweepDecision, parse_evolve_response, parse_sweep_response,
};
pub use opengoose_skills::evolution::prompts::{
    UpdatePromptParams, build_evolve_prompt, build_sweep_prompt, build_update_prompt,
    summarize_for_prompt,
};
pub use opengoose_skills::evolution::validator::validate_skill_output;
pub use opengoose_skills::evolution::writer::{
    WriteSkillParams, refine_skill, update_effectiveness_versioned, update_existing_skill,
    write_skill_to_rig_scope,
};
pub use opengoose_skills::metadata::SkillMetadata;

// ---------------------------------------------------------------------------
// read_conversation_log — depends on opengoose_rig (stays in binary crate)
// ---------------------------------------------------------------------------

pub fn read_conversation_log(work_item_id: i64) -> String {
    let session_id = format!("task-{work_item_id}");
    opengoose_rig::conversation_log::read_log(&session_id)
        .map(|content| summarize_for_prompt(&content, 4000))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use opengoose_skills::metadata::{Effectiveness, GeneratedFrom};

    /// Build a minimal SkillMetadata for tests. Only stamp_id and skill_version vary.
    fn test_metadata(stamp_id: i64, skill_version: u32) -> SkillMetadata {
        SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version,
        }
    }

    /// Build a JSON map of active skill name → version from loaded skills.
    fn build_active_versions_json(skills: &[crate::skills::load::LoadedSkill]) -> String {
        let mut map = std::collections::HashMap::new();
        for skill in skills {
            if skill.scope == crate::skills::load::SkillScope::Learned
                && let Some(meta) = crate::skills::load::read_metadata(&skill.path)
            {
                map.insert(skill.name.clone(), meta.skill_version);
            }
        }
        serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
    }

    #[test]
    fn parse_evolve_response_skip_returns_skip_action() {
        assert_eq!(parse_evolve_response("SKIP"), EvolveAction::Skip);
    }

    #[test]
    fn parse_evolve_response_update_returns_skill_name() {
        assert_eq!(
            parse_evolve_response("UPDATE:existing-skill"),
            EvolveAction::Update("existing-skill".into())
        );
    }

    #[test]
    fn parse_evolve_response_frontmatter_returns_create_action() {
        let content = "---\nname: test\ndescription: Use when testing\n---\n# Body\n";
        match parse_evolve_response(content) {
            EvolveAction::Create(c) => assert!(c.contains("test")),
            _ => panic!("expected Create"),
        }
    }

    #[test]
    fn validate_skill_output_accepts_valid_frontmatter() {
        let content = "---\nname: test-skill\ndescription: Use when testing code\n---\n# Body\n";
        assert!(validate_skill_output(content).is_ok());
    }

    #[test]
    fn validate_skill_output_rejects_missing_frontmatter() {
        assert!(validate_skill_output("# No frontmatter").is_err());
    }

    #[test]
    fn validate_skill_output_rejects_description_without_use_when() {
        let content = "---\nname: test\ndescription: This does things\n---\n";
        assert!(validate_skill_output(content).is_err());
    }

    #[test]
    fn validate_skill_output_rejects_uppercase_underscore_name() {
        let content = "---\nname: Test_Skill\ndescription: Use when testing\n---\n";
        assert!(validate_skill_output(content).is_err());
    }

    #[test]
    fn build_evolve_prompt_includes_dimension_score_comment_title() {
        let prompt = build_evolve_prompt("Quality", 0.2, Some("no tests"), "Fix auth", 42, "", &[]);
        assert!(prompt.contains("Quality"));
        assert!(prompt.contains("0.2"));
        assert!(prompt.contains("no tests"));
        assert!(prompt.contains("Fix auth"));
    }

    #[test]
    fn build_evolve_prompt_lists_existing_skills_section() {
        let existing = vec![("skill-a".into(), "desc-a".into())];
        let prompt = build_evolve_prompt("Quality", 0.2, None, "task", 1, "", &existing);
        assert!(prompt.contains("skill-a"));
        assert!(prompt.contains("Existing Skills"));
    }

    #[test]
    fn build_update_prompt_includes_existing_skill_content() {
        let existing_content = "---\nname: validate-paths\ndescription: Use when reading files\n---\nAlways check paths.\n";
        let prompt = build_update_prompt(&UpdatePromptParams {
            skill_name: "validate-paths",
            existing_content,
            dimension: "Quality",
            score: 0.1,
            comment: Some("path traversal"),
            work_item_title: "Fix file reader",
            work_item_id: 55,
            log_summary: "log excerpt...",
        });
        assert!(prompt.contains("validate-paths"));
        assert!(prompt.contains("Always check paths"));
        assert!(prompt.contains("path traversal"));
        assert!(prompt.contains("UPDATE this skill"));
    }

    #[test]
    fn parse_skill_header_extracts_name_from_frontmatter() {
        let content = "---\nname: my-skill\ndescription: Use when testing\n---\n# Body\n";
        let name = opengoose_rig::middleware::parse_skill_header(content)
            .map(|(n, _)| n)
            .expect("valid frontmatter should parse successfully");
        assert_eq!(name, "my-skill");
    }

    #[test]
    fn metadata_serde_roundtrip_preserves_fields() {
        let mut meta = test_metadata(5, 1);
        meta.generated_from.work_item_id = 42;
        meta.generated_at = "2026-03-19T10:00:00Z".into();
        meta.evolver_work_item_id = Some(100);
        let json = serde_json::to_string(&meta).expect("metadata should serialize to JSON");
        let parsed: SkillMetadata =
            serde_json::from_str(&json).expect("metadata should deserialize from JSON");
        assert_eq!(parsed.generated_from.stamp_id, 5);
        assert_eq!(parsed.evolver_work_item_id, Some(100));
    }

    #[test]
    fn summarize_truncates_long_content() {
        let content = "a".repeat(10000);
        let summary = summarize_for_prompt(&content, 4000);
        assert_eq!(summary.len(), 4000);
    }

    #[test]
    fn update_effectiveness_versioned_matching_version() {
        let tmp = tempfile::tempdir().expect("tempdir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir creation should succeed");

        let meta = test_metadata(1, 2);
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("metadata should serialize"),
        )
        .expect("writing metadata.json should succeed");

        // Version matches → score should be added
        let versions = r#"{"my-skill": 2}"#;
        update_effectiveness_versioned(&skill_dir, 0.8, Some(versions))
            .expect("versioned effectiveness update should succeed");

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("reading metadata.json should succeed"),
        )
        .expect("metadata.json should be valid JSON");
        assert_eq!(updated.effectiveness.subsequent_scores, vec![0.8]);
    }

    #[test]
    fn update_effectiveness_versioned_old_version_ignored() {
        let tmp = tempfile::tempdir().expect("tempdir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir creation should succeed");

        let meta = test_metadata(1, 2);
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("metadata should serialize"),
        )
        .expect("writing metadata.json should succeed");

        // Old version → score should be ignored
        let old_versions = r#"{"my-skill": 1}"#;
        update_effectiveness_versioned(&skill_dir, 0.8, Some(old_versions))
            .expect("versioned effectiveness update should succeed");

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("reading metadata.json should succeed"),
        )
        .expect("metadata.json should be valid JSON");
        assert!(updated.effectiveness.subsequent_scores.is_empty());
    }

    #[test]
    fn update_effectiveness_versioned_no_versions_legacy() {
        let tmp = tempfile::tempdir().expect("tempdir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir creation should succeed");

        let meta = test_metadata(1, 1);
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("metadata should serialize"),
        )
        .expect("writing metadata.json should succeed");

        // No version info (legacy) → score should always count
        update_effectiveness_versioned(&skill_dir, 0.7, None)
            .expect("legacy effectiveness update should succeed");

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("reading metadata.json should succeed"),
        )
        .expect("metadata.json should be valid JSON");
        assert_eq!(updated.effectiveness.subsequent_scores, vec![0.7]);
    }

    #[test]
    fn update_existing_skill_overwrites_content() {
        let tmp = tempfile::tempdir().expect("tempdir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir creation should succeed");

        let original = "---\nname: my-skill\ndescription: Use when original\n---\nOld body\n";
        std::fs::write(skill_dir.join("SKILL.md"), original)
            .expect("writing SKILL.md should succeed");

        let mut meta = test_metadata(1, 1);
        meta.effectiveness.injected_count = 2;
        meta.effectiveness.subsequent_scores = vec![0.3, 0.4];
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("metadata should serialize"),
        )
        .expect("writing metadata.json should succeed");

        let new_content = "---\nname: my-skill\ndescription: Use when updated\n---\nNew body\n";
        update_existing_skill(&skill_dir, new_content, 5, 42, "Quality", 0.15, Some(100)).unwrap();

        let written = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(written.contains("New body"));

        let updated_meta: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated_meta.generated_from.stamp_id, 5);
        assert!(updated_meta.effectiveness.subsequent_scores.is_empty());
        assert_eq!(updated_meta.skill_version, 2);
    }

    #[test]
    fn build_active_versions_json_maps_learned_skill_versions() {
        use crate::skills::load::{LoadedSkill, SkillScope};

        let tmp = tempfile::tempdir().expect("tempdir creation should succeed");
        let skill_dir = tmp.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir creation should succeed");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: test-skill\ndescription: Use when test\n---\n",
        )
        .expect("writing SKILL.md should succeed");

        let meta = test_metadata(1, 3);
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("metadata should serialize"),
        )
        .expect("writing metadata.json should succeed");

        let skills = vec![LoadedSkill {
            name: "test-skill".into(),
            description: "Use when test".into(),
            path: skill_dir,
            content: String::new(),
            scope: SkillScope::Learned,
        }];

        let json = build_active_versions_json(&skills);
        let parsed: std::collections::HashMap<String, u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.get("test-skill"), Some(&3));
    }

    #[test]
    fn refine_skill_bumps_version_preserves_generated_from() {
        let tmp = tempfile::tempdir().expect("tempdir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("skill dir creation should succeed");

        let original = "---\nname: my-skill\ndescription: Use when original\n---\nOld body\n";
        std::fs::write(skill_dir.join("SKILL.md"), original)
            .expect("writing SKILL.md should succeed");

        let mut meta = test_metadata(99, 3);
        meta.generated_from.work_item_id = 50;
        meta.generated_from.score = 0.15;
        meta.evolver_work_item_id = Some(200);
        meta.last_included_at = Some(Utc::now().to_rfc3339());
        meta.effectiveness.injected_count = 5;
        meta.effectiveness.subsequent_scores = vec![0.4, 0.5];
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).expect("metadata should serialize"),
        )
        .expect("writing metadata.json should succeed");

        let new_content = "---\nname: my-skill\ndescription: Use when refined\n---\nNew body\n";
        refine_skill(&skill_dir, new_content).unwrap();

        let written = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(written.contains("New body"));

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        // generated_from preserved
        assert_eq!(updated.generated_from.stamp_id, 99);
        assert_eq!(updated.evolver_work_item_id, Some(200));
        // version bumped, effectiveness reset
        assert_eq!(updated.skill_version, 4);
        assert!(updated.effectiveness.subsequent_scores.is_empty());
        assert!(updated.last_included_at.is_none());
    }

    #[test]
    fn build_sweep_prompt_includes_skills_and_failures() {
        let dormant_skills = vec![(
            "validate-paths".to_string(),
            "Use when reading files".to_string(),
            "Always check paths exist.".to_string(),
            None::<String>,
        )];
        let recent_failures = vec![
            "stamp #5: Quality 0.1 on 'File reader crashed on missing path'".to_string(),
            "stamp #8: Quality 0.2 on 'Path traversal vulnerability found'".to_string(),
        ];
        let prompt = build_sweep_prompt(&dormant_skills, &recent_failures);
        assert!(prompt.contains("validate-paths"));
        assert!(prompt.contains("File reader crashed"));
        assert!(prompt.contains("RESTORE"));
        assert!(prompt.contains("DELETE"));
    }

    #[test]
    fn sweep_prompt_includes_effectiveness() {
        let skills = vec![(
            "fix-auth".to_string(),
            "Use when auth fails".to_string(),
            "Check token expiry".to_string(),
            Some("8 injections, avg score 0.22, verdict: ineffective".to_string()),
        )];
        let failures = vec!["stamp #5: Quality 0.2 on 'missed validation'".to_string()];
        let prompt = build_sweep_prompt(&skills, &failures);
        assert!(prompt.contains("8 injections"));
        assert!(prompt.contains("ineffective"));
        assert!(prompt.contains("Effectiveness"));
    }

    #[test]
    fn parse_sweep_response_extracts_decisions() {
        let response = "RESTORE:validate-paths\nDELETE:old-generic-skill\nKEEP:maybe-useful\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 3);
        assert_eq!(
            decisions[0],
            SweepDecision::Restore("validate-paths".into())
        );
        assert_eq!(
            decisions[1],
            SweepDecision::Delete("old-generic-skill".into())
        );
        assert_eq!(decisions[2], SweepDecision::Keep("maybe-useful".into()));
    }

    #[test]
    fn parse_sweep_response_refine_with_content() {
        let response =
            "REFINE:my-skill\n---\nname: my-skill\ndescription: Use when updated\n---\nNew body\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 1);
        match &decisions[0] {
            SweepDecision::Refine(name, content) => {
                assert_eq!(name, "my-skill");
                assert!(content.contains("Use when updated"));
            }
            _ => panic!("expected Refine"),
        }
    }

    #[test]
    fn parse_sweep_response_mixed_with_refine() {
        let response = "RESTORE:skill-a\nREFINE:skill-b\n---\nname: skill-b\ndescription: Use when b\n---\nbody\nDELETE:skill-c\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 3);
        assert_eq!(decisions[0], SweepDecision::Restore("skill-a".into()));
        match &decisions[1] {
            SweepDecision::Refine(name, _) => assert_eq!(name, "skill-b"),
            _ => panic!("expected Refine"),
        }
        assert_eq!(decisions[2], SweepDecision::Delete("skill-c".into()));
    }

    #[test]
    fn parse_sweep_response_empty_input_returns_no_decisions() {
        let decisions = parse_sweep_response("");
        assert!(decisions.is_empty());
    }

    #[test]
    fn validate_skill_output_rejects_name_exceeding_64_chars() {
        let long_name = "a".repeat(65);
        let content = format!("---\nname: {long_name}\ndescription: Use when testing\n---\n");
        assert!(validate_skill_output(&content).is_err());
    }

    #[test]
    fn validate_skill_output_rejects_unclosed_frontmatter() {
        let content = "---\nname: test\ndescription: Use when testing";
        assert!(validate_skill_output(content).is_err());
    }

    #[test]
    fn validate_skill_output_rejects_missing_name_field() {
        let content = "---\ndescription: Use when testing\n---\n";
        assert!(validate_skill_output(content).is_err());
    }

    #[test]
    fn build_evolve_prompt_with_log_summary() {
        let prompt = build_evolve_prompt(
            "Quality",
            0.2,
            None,
            "Fix auth",
            42,
            "user: help\nassistant: ok",
            &[],
        );
        assert!(prompt.contains("Conversation Log"));
        assert!(prompt.contains("user: help"));
    }

    #[test]
    fn write_skill_to_rig_scope_creates_files() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        let home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", tmp.path());
        }
        std::env::set_current_dir(tmp.path()).unwrap();

        let content = "---\nname: new-skill\ndescription: Use when testing\n---\n# Body\n";
        let name = write_skill_to_rig_scope(
            tmp.path(),
            "rig-1",
            content,
            WriteSkillParams {
                stamp_id: 1,
                work_item_id: 42,
                dimension: "Quality",
                score: 0.2,
                evolver_work_item_id: None,
            },
        )
        .unwrap();
        assert_eq!(name, "new-skill");

        let skill_path = tmp
            .path()
            .join(".opengoose/rigs/rig-1/skills/learned/new-skill");
        assert!(skill_path.join("SKILL.md").is_file());
        assert!(skill_path.join("metadata.json").is_file());

        unsafe {
            match home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
        std::env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn write_skill_fails_for_no_name_in_content() {
        let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        let home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", tmp.path());
        }

        let content = "No frontmatter here";
        let result = write_skill_to_rig_scope(
            tmp.path(),
            "rig-1",
            content,
            WriteSkillParams {
                stamp_id: 1,
                work_item_id: 42,
                dimension: "Quality",
                score: 0.2,
                evolver_work_item_id: None,
            },
        );
        assert!(result.is_err());

        unsafe {
            match home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
        std::env::set_current_dir(cwd).unwrap();
    }

    #[test]
    fn refine_skill_without_metadata_only_writes_skill_md() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("no-meta-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        // No metadata.json present

        let new_content = "---\nname: no-meta-skill\ndescription: Use when no meta\n---\nBody\n";
        refine_skill(&skill_dir, new_content).unwrap();

        assert!(skill_dir.join("SKILL.md").is_file());
        // No metadata.json created (prev was None → skip update)
        assert!(!skill_dir.join("metadata.json").is_file());
    }

    #[test]
    fn update_existing_skill_without_prior_metadata_uses_version_1() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("fresh-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        // No metadata.json, no SKILL.md

        let new_content = "---\nname: fresh-skill\ndescription: Use when fresh\n---\nBody\n";
        update_existing_skill(&skill_dir, new_content, 1, 1, "Quality", 0.5, None).unwrap();

        let meta: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        // prev_version was None → unwrap_or(1) → version becomes 1+1=2
        assert_eq!(meta.skill_version, 2);
    }

    #[test]
    fn read_conversation_log_returns_empty_when_missing() {
        // work_item_id with no corresponding log file → returns empty string
        let result = read_conversation_log(99999);
        assert!(result.is_empty());
    }

    #[test]
    fn build_active_versions_json_skips_installed_skills() {
        use crate::skills::load::{LoadedSkill, SkillScope};

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("installed-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        // Installed scope → should be skipped, not included in the map
        let skills = vec![LoadedSkill {
            name: "installed-skill".into(),
            description: "Use when installed".into(),
            path: skill_dir,
            content: String::new(),
            scope: SkillScope::Installed,
        }];

        let json = build_active_versions_json(&skills);
        let parsed: std::collections::HashMap<String, u32> = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_empty());
    }
}
