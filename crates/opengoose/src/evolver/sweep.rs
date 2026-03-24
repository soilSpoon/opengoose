// Sweep logic — offline re-evaluation of dormant/archived skills.

use super::{AgentCaller, LOW_STAMP_THRESHOLD};
use crate::skills::{evolve, load};
use opengoose_board::Board;
use tracing::info;

// ---------------------------------------------------------------------------
// Decision execution — independently testable
// ---------------------------------------------------------------------------

/// Apply a single sweep decision to a dormant skill.
/// Returns Ok(true) if the decision was applied, Ok(false) if skipped (e.g., skill not found).
fn apply_decision(
    decision: &evolve::SweepDecision,
    dormant: &[load::LoadedSkill],
) -> anyhow::Result<bool> {
    match decision {
        evolve::SweepDecision::Restore(name) => {
            if let Some(skill) = dormant.iter().find(|s| &s.name == name) {
                load::update_inclusion_tracking(&skill.path);
                info!("sweep: restored '{name}' to active");
                Ok(true)
            } else {
                Ok(false)
            }
        }
        evolve::SweepDecision::Refine(name, content) => {
            if let Some(skill) = dormant.iter().find(|s| &s.name == name)
                && evolve::validate_skill_output(content).is_ok()
            {
                evolve::refine_skill(&skill.path, content)?;
                load::update_inclusion_tracking(&skill.path);
                info!("sweep: refined and restored '{name}'");
                Ok(true)
            } else {
                Ok(false)
            }
        }
        evolve::SweepDecision::Delete(name) => {
            if let Some(skill) = dormant.iter().find(|s| &s.name == name) {
                std::fs::remove_dir_all(&skill.path)?;
                info!("sweep: deleted obsolete skill '{name}'");
                Ok(true)
            } else {
                Ok(false)
            }
        }
        evolve::SweepDecision::Keep(name) => {
            info!("sweep: keeping '{name}' dormant");
            Ok(true)
        }
    }
}

/// Build effectiveness summary string for a skill's metadata.
fn build_effectiveness_summary(meta: &opengoose_skills::metadata::SkillMetadata) -> String {
    let scores = &meta.effectiveness.subsequent_scores;
    let avg = if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f32>() / scores.len() as f32
    };
    let verdict = match load::is_effective(meta) {
        Some(true) => "effective",
        Some(false) => "ineffective",
        None => "insufficient data",
    };
    format!(
        "{} injections, avg score {:.2}, verdict: {}",
        meta.effectiveness.injected_count, avg, verdict
    )
}

/// Idle-time sweep: re-evaluate dormant/archived skills against recent failures.
pub(super) async fn run_sweep(
    board: &Board,
    caller: &dyn AgentCaller,
) -> anyhow::Result<()> {
    let home = crate::home_dir();
    let global_dir = home.join(".opengoose/skills");
    let rigs_base = home.join(".opengoose/rigs");

    // 1. Load dormant/archived skills
    let dormant = load::load_dormant_and_archived(&global_dir, None, &rigs_base);
    if dormant.is_empty() {
        return Ok(());
    }

    // 2. Get recent low stamps (last 30 days) for failure context
    let recent_stamps = board.recent_low_stamps(LOW_STAMP_THRESHOLD, 30).await?;
    if recent_stamps.is_empty() {
        return Ok(()); // no recent failures to compare against
    }

    let failure_summaries: Vec<String> = recent_stamps
        .iter()
        .map(|s| {
            format!(
                "stamp #{}: {} {:.1} on '{}'",
                s.id,
                s.dimension,
                s.score,
                s.comment.as_deref().unwrap_or("(no comment)")
            )
        })
        .collect();

    let skill_summaries: Vec<(String, String, String, Option<String>)> = dormant
        .iter()
        .map(|s| {
            let body = load::extract_body(&s.content)
                .map(|b| evolve::summarize_for_prompt(b, 300))
                .unwrap_or_default();
            let effectiveness =
                load::read_metadata(&s.path).map(|meta| build_effectiveness_summary(&meta));
            (s.name.clone(), s.description.clone(), body, effectiveness)
        })
        .collect();

    // 3. Build and send sweep prompt
    let prompt = evolve::build_sweep_prompt(&skill_summaries, &failure_summaries);
    let response = caller.call(&prompt, 0).await?;

    // 4. Parse and apply decisions
    let decisions = evolve::parse_sweep_response(&response);
    for decision in &decisions {
        let _ = apply_decision(decision, &dormant);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]
    use super::*;
    use crate::skills::test_env_lock;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use opengoose_board::Board;
    use opengoose_board::board::AddStampParams;
    use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
    use std::ffi::OsString;
    use tempfile::tempdir;

    fn set_env_var(key: &str, value: Option<&str>) -> Option<OsString> {
        let prev = std::env::var_os(key);
        unsafe {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        prev
    }

    fn restore_env_var(key: &str, prev: Option<OsString>) {
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    struct MockAgentCaller {
        reply: String,
    }

    #[async_trait]
    impl super::super::AgentCaller for MockAgentCaller {
        async fn call(&self, _prompt: &str, _work_id: i64) -> anyhow::Result<String> {
            Ok(self.reply.clone())
        }
    }

    fn dormant_skill(
        base_home: &std::path::Path,
        rig: &str,
        name: &str,
        generated_days_ago: i64,
    ) -> std::path::PathBuf {
        let skill_dir = base_home
            .join(".opengoose/rigs")
            .join(rig)
            .join("skills/learned")
            .join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: Use when testing old behavior\n---\n\n# {name}\n"
            ),
        )
        .unwrap();

        let meta = crate::skills::evolve::SkillMetadata {
            generated_from: opengoose_skills::metadata::GeneratedFrom {
                stamp_id: 1,
                work_item_id: 2,
                dimension: "Quality".into(),
                score: 0.1,
            },
            generated_at: (Utc::now() - Duration::days(generated_days_ago)).to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: opengoose_skills::metadata::Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![0.0, 0.0, 0.0],
            },
            skill_version: 1,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_vec_pretty(&meta).unwrap(),
        )
        .unwrap();
        skill_dir
    }

    /// Helper: creates a board with one recent stamp so run_sweep proceeds past the early-exit.
    async fn board_with_recent_stamp() -> Board {
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let item = board
            .post(PostWorkItem {
                title: "recent failure for sweep".into(),
                description: String::new(),
                created_by: RigId::new("evolver"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .add_stamp(AddStampParams {
                target_rig: "sweep-rig",
                work_item_id: item.id,
                dimension: "Quality",
                score: 0.1,
                severity: "Leaf",
                stamped_by: "human",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .unwrap();
        board
    }

    /// Creates a single dormant skill with custom effectiveness scores.
    fn dormant_skill_custom(
        base_home: &std::path::Path,
        rig: &str,
        name: &str,
        scores: Vec<f32>,
    ) -> std::path::PathBuf {
        let skill_dir = base_home
            .join(".opengoose/rigs")
            .join(rig)
            .join("skills/learned")
            .join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Use when testing\n---\n# {name}\n"),
        )
        .unwrap();
        let meta = crate::skills::evolve::SkillMetadata {
            generated_from: opengoose_skills::metadata::GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.1,
            },
            generated_at: (Utc::now() - Duration::days(61)).to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: opengoose_skills::metadata::Effectiveness {
                injected_count: scores.len() as u32,
                subsequent_scores: scores,
            },
            skill_version: 1,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_vec_pretty(&meta).unwrap(),
        )
        .unwrap();
        skill_dir
    }

    #[tokio::test]
    async fn run_sweep_returns_ok_when_no_dormant_skills() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: String::new(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();

        run_sweep(&board, &caller).await.unwrap();

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn run_sweep_deletes_dormant_skill_from_decision() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "DELETE:dormant-old".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(PostWorkItem {
                title: "old failure".into(),
                description: String::new(),
                created_by: RigId::new("evolver"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .add_stamp(AddStampParams {
                target_rig: "r-dormant",
                work_item_id: work_item.id,
                dimension: "Quality",
                score: 0.1,
                severity: "Leaf",
                stamped_by: "human",
                comment: Some("recent failure"),
                active_skill_versions: None,
            })
            .await
            .unwrap();

        let path = dormant_skill(home.path(), "r-dormant", "dormant-old", 60);
        assert!(path.is_dir());

        run_sweep(&board, &caller).await.unwrap();
        assert!(!path.exists());

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn run_sweep_keeps_dormant_skill_from_decision() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "KEEP:dormant-keep".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(PostWorkItem {
                title: "keep failure".into(),
                description: String::new(),
                created_by: RigId::new("evolver"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .add_stamp(AddStampParams {
                target_rig: "r-keep",
                work_item_id: work_item.id,
                dimension: "Quality",
                score: 0.1,
                severity: "Leaf",
                stamped_by: "human",
                comment: Some("recent failure"),
                active_skill_versions: None,
            })
            .await
            .unwrap();

        let path = dormant_skill(home.path(), "r-keep", "dormant-keep", 60);
        assert!(path.is_dir());

        run_sweep(&board, &caller).await.unwrap();
        assert!(path.exists());

        let metadata: crate::skills::evolve::SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(path.join("metadata.json")).unwrap())
                .unwrap();
        assert!(metadata.last_included_at.is_none());

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn run_sweep_restores_dormant_skill_from_decision() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "RESTORE:dormant-restore".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(PostWorkItem {
                title: "restore failure".into(),
                description: String::new(),
                created_by: RigId::new("evolver"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .add_stamp(AddStampParams {
                target_rig: "r-restore",
                work_item_id: work_item.id,
                dimension: "Quality",
                score: 0.1,
                severity: "Leaf",
                stamped_by: "human",
                comment: Some("recent failure"),
                active_skill_versions: None,
            })
            .await
            .unwrap();

        let path = dormant_skill(home.path(), "r-restore", "dormant-restore", 60);
        let metadata_before: crate::skills::evolve::SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(path.join("metadata.json")).unwrap())
                .unwrap();
        assert!(metadata_before.last_included_at.is_none());

        run_sweep(&board, &caller).await.unwrap();

        let metadata_after: crate::skills::evolve::SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(path.join("metadata.json")).unwrap())
                .unwrap();
        assert!(metadata_after.last_included_at.is_some());

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn run_sweep_refines_dormant_skill_from_decision() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "REFINE:dormant-refine\n---\nname: dormant-refine\ndescription: Use when testing old behavior\n---\n\n# refind".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(PostWorkItem {
                title: "refine failure".into(),
                description: String::new(),
                created_by: RigId::new("evolver"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .add_stamp(AddStampParams {
                target_rig: "r-refine",
                work_item_id: work_item.id,
                dimension: "Quality",
                score: 0.1,
                severity: "Leaf",
                stamped_by: "human",
                comment: Some("recent failure"),
                active_skill_versions: None,
            })
            .await
            .unwrap();

        let path = dormant_skill(home.path(), "r-refine", "dormant-refine", 60);
        let metadata_before: crate::skills::evolve::SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(path.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(metadata_before.skill_version, 1);

        run_sweep(&board, &caller).await.unwrap();
        let content = std::fs::read_to_string(path.join("SKILL.md")).unwrap();
        assert!(content.contains("# refind"));

        let metadata_after: crate::skills::evolve::SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(path.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(metadata_after.skill_version, 2);
        assert!(metadata_after.last_included_at.is_some());

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// run_sweep returns early when dormant skills exist but there are no recent low stamps.
    #[tokio::test]
    async fn run_sweep_returns_ok_when_dormant_exists_but_no_recent_stamps() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        // Create a dormant skill so it passes the first early-return check.
        let path = dormant_skill(home.path(), "r-nostamp", "dormant-nostamp", 60);
        assert!(path.is_dir());

        // Board has no stamps → recent_low_stamps returns empty → early return.
        let caller = MockAgentCaller {
            reply: String::new(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        run_sweep(&board, &caller).await.unwrap();

        // Skill should still exist (sweep returned before making decisions).
        assert!(path.is_dir());

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// Covers evolver — all branches of the effectiveness verdict in run_sweep.
    #[tokio::test]
    async fn run_sweep_covers_effectiveness_branch_variants() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "KEEP:skill-empty\nKEEP:skill-insufficient\nKEEP:skill-effective".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(PostWorkItem {
                title: "effectiveness context failure".into(),
                description: String::new(),
                created_by: RigId::new("evolver"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .add_stamp(AddStampParams {
                target_rig: "eff-rig",
                work_item_id: work_item.id,
                dimension: "Quality",
                score: 0.1,
                severity: "Leaf",
                stamped_by: "human",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .unwrap();

        let rigs_dir = home.path().join(".opengoose/rigs/eff-rig/skills/learned");

        // Skill A: empty subsequent_scores → scores.is_empty() = true
        let skill_a = rigs_dir.join("skill-empty");
        std::fs::create_dir_all(&skill_a).unwrap();
        std::fs::write(
            skill_a.join("SKILL.md"),
            "---\nname: skill-empty\ndescription: Use when testing empty scores\n---\n# Empty\n",
        )
        .unwrap();
        let meta_a = crate::skills::evolve::SkillMetadata {
            generated_from: opengoose_skills::metadata::GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.1,
            },
            generated_at: (Utc::now() - Duration::days(61)).to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: opengoose_skills::metadata::Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 1,
        };
        std::fs::write(
            skill_a.join("metadata.json"),
            serde_json::to_vec_pretty(&meta_a).unwrap(),
        )
        .unwrap();

        // Skill B: 2 scores → is_effective returns None → "insufficient data"
        let skill_b = rigs_dir.join("skill-insufficient");
        std::fs::create_dir_all(&skill_b).unwrap();
        std::fs::write(
            skill_b.join("SKILL.md"),
            "---\nname: skill-insufficient\ndescription: Use when testing insufficient data\n---\n# Insufficient\n",
        )
        .unwrap();
        let meta_b = crate::skills::evolve::SkillMetadata {
            generated_from: opengoose_skills::metadata::GeneratedFrom {
                stamp_id: 2,
                work_item_id: 2,
                dimension: "Quality".into(),
                score: 0.1,
            },
            generated_at: (Utc::now() - Duration::days(61)).to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: opengoose_skills::metadata::Effectiveness {
                injected_count: 1,
                subsequent_scores: vec![0.5, 0.6],
            },
            skill_version: 1,
        };
        std::fs::write(
            skill_b.join("metadata.json"),
            serde_json::to_vec_pretty(&meta_b).unwrap(),
        )
        .unwrap();

        // Skill C: 3 high scores + low initial → is_effective = Some(true) → "effective"
        let skill_c = rigs_dir.join("skill-effective");
        std::fs::create_dir_all(&skill_c).unwrap();
        std::fs::write(
            skill_c.join("SKILL.md"),
            "---\nname: skill-effective\ndescription: Use when testing effective verdict\n---\n# Effective\n",
        )
        .unwrap();
        let meta_c = crate::skills::evolve::SkillMetadata {
            generated_from: opengoose_skills::metadata::GeneratedFrom {
                stamp_id: 3,
                work_item_id: 3,
                dimension: "Quality".into(),
                score: 0.1,
            },
            generated_at: (Utc::now() - Duration::days(61)).to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: opengoose_skills::metadata::Effectiveness {
                injected_count: 3,
                subsequent_scores: vec![0.7, 0.8, 0.9],
            },
            skill_version: 1,
        };
        std::fs::write(
            skill_c.join("metadata.json"),
            serde_json::to_vec_pretty(&meta_c).unwrap(),
        )
        .unwrap();

        run_sweep(&board, &caller).await.unwrap();

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// Covers evolver — RESTORE for a skill not in the dormant list → skips.
    #[tokio::test]
    async fn run_sweep_restore_nonexistent_skill_skips_gracefully() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "RESTORE:not-a-real-skill".into(),
        };
        let board = board_with_recent_stamp().await;
        dormant_skill(home.path(), "skip-rig", "real-skill", 60);

        // Should succeed (no error for missing skill, just skip)
        run_sweep(&board, &caller).await.unwrap();

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// Covers evolver — REFINE for a skill not in the dormant list → skips.
    #[tokio::test]
    async fn run_sweep_refine_nonexistent_skill_skips() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "REFINE:ghost-skill\n---\nname: ghost-skill\ndescription: Use when ghost\n---\n# Ghost\n".into(),
        };
        let board = board_with_recent_stamp().await;
        dormant_skill(home.path(), "skip-rig2", "real-skill-2", 60);

        run_sweep(&board, &caller).await.unwrap();

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// Covers evolver — REFINE with invalid content (validation fails) → skips.
    #[tokio::test]
    async fn run_sweep_refine_invalid_content_skips() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        // Content without frontmatter → validate_skill_output fails
        let caller = MockAgentCaller {
            reply: "REFINE:refine-target\njust some content without frontmatter".into(),
        };
        let board = board_with_recent_stamp().await;
        dormant_skill_custom(
            home.path(),
            "skip-rig3",
            "refine-target",
            vec![0.0, 0.0, 0.0],
        );

        run_sweep(&board, &caller).await.unwrap();

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// Covers evolver — DELETE for a skill not in the dormant list → skips.
    #[tokio::test]
    async fn run_sweep_delete_nonexistent_skill_skips() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "DELETE:nonexistent-skill".into(),
        };
        let board = board_with_recent_stamp().await;
        dormant_skill(home.path(), "skip-rig4", "real-skill-4", 60);

        run_sweep(&board, &caller).await.unwrap();

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    // --- Pure function tests for apply_decision ---

    #[test]
    fn apply_decision_keep_returns_true() {
        let decision = evolve::SweepDecision::Keep("some-skill".into());
        let result = apply_decision(&decision, &[]);
        assert!(result.unwrap());
    }

    #[test]
    fn apply_decision_restore_nonexistent_returns_false() {
        let decision = evolve::SweepDecision::Restore("missing".into());
        let result = apply_decision(&decision, &[]);
        assert!(!result.unwrap());
    }

    #[test]
    fn apply_decision_delete_nonexistent_returns_false() {
        let decision = evolve::SweepDecision::Delete("missing".into());
        let result = apply_decision(&decision, &[]);
        assert!(!result.unwrap());
    }

    #[test]
    fn apply_decision_refine_nonexistent_returns_false() {
        let decision = evolve::SweepDecision::Refine("missing".into(), "content".into());
        let result = apply_decision(&decision, &[]);
        assert!(!result.unwrap());
    }

    #[test]
    fn build_effectiveness_summary_empty_scores() {
        let meta = opengoose_skills::metadata::SkillMetadata {
            generated_from: opengoose_skills::metadata::GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.1,
            },
            generated_at: String::new(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: opengoose_skills::metadata::Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 1,
        };
        let summary = build_effectiveness_summary(&meta);
        assert!(summary.contains("0 injections"));
        assert!(summary.contains("avg score 0.00"));
    }

    #[test]
    fn build_effectiveness_summary_with_scores() {
        let meta = opengoose_skills::metadata::SkillMetadata {
            generated_from: opengoose_skills::metadata::GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.1,
            },
            generated_at: String::new(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: opengoose_skills::metadata::Effectiveness {
                injected_count: 3,
                subsequent_scores: vec![0.7, 0.8, 0.9],
            },
            skill_version: 1,
        };
        let summary = build_effectiveness_summary(&meta);
        assert!(summary.contains("3 injections"));
        assert!(summary.contains("avg score 0.80"));
    }
}
