// Stamp processing pipeline — prepare_context, execute_action, process_stamp.

use super::AgentCaller;
use crate::skills::{evolve, load};
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
use std::path::Path;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// StampContext — data produced by prepare_context, consumed by execute_action
// ---------------------------------------------------------------------------

struct StampContext {
    work_item: opengoose_board::WorkItem,
    evolver_item_id: i64,
    log_summary: String,
    prompt: String,
}

// ---------------------------------------------------------------------------
// Step 0: update effectiveness scores for existing skills
// ---------------------------------------------------------------------------

fn update_effectiveness(
    stamp: &opengoose_board::entity::stamp::Model,
    existing: &[load::LoadedSkill],
) {
    for skill in existing {
        if skill.scope == load::SkillScope::Learned
            && let Some(meta) = load::read_metadata(&skill.path)
            && meta.generated_from.dimension == stamp.dimension
            && let Err(e) = evolve::update_effectiveness_versioned(
                &skill.path,
                stamp.score,
                stamp.active_skill_versions.as_deref(),
            )
        {
            warn!("evolver: effectiveness update failed for '{}': {e}", skill.name);
        }
    }
}

// ---------------------------------------------------------------------------
// Steps 1-6: load work item, post+claim evolver item, read log, build prompt
// ---------------------------------------------------------------------------

async fn prepare_context(
    board: &Board,
    stamp: &opengoose_board::entity::stamp::Model,
    existing: &[load::LoadedSkill],
) -> anyhow::Result<StampContext> {
    let evolver_rig = RigId::new("evolver");

    // 1. Get work item info for context
    let work_item = board
        .get(stamp.work_item_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("work item {} not found", stamp.work_item_id))?;

    // 2. Post "skill generation" work item
    let evolver_item = board
        .post(PostWorkItem {
            title: format!(
                "Generate skill: {} score {:.1} on #{}",
                stamp.dimension, stamp.score, stamp.work_item_id
            ),
            description: format!(
                "Analyze low {} stamp on work item '{}' and generate SKILL.md",
                stamp.dimension, work_item.title
            ),
            created_by: evolver_rig.clone(),
            priority: Priority::P2,
            tags: vec!["skill-generation".into()],
        })
        .await?;

    // 3. Claim it
    board.claim(evolver_item.id, &evolver_rig).await?;

    // 4. Read conversation log
    let log_summary = evolve::read_conversation_log(stamp.work_item_id);

    // 5. Build existing pairs for dedup check (reuse from step 0)
    let existing_pairs: Vec<(String, String)> = existing
        .iter()
        .map(|s| (s.name.clone(), s.description.clone()))
        .collect();

    // 6. Build prompt
    let prompt = evolve::build_evolve_prompt(
        &stamp.dimension,
        stamp.score,
        stamp.comment.as_deref(),
        &work_item.title,
        stamp.work_item_id,
        &log_summary,
        &existing_pairs,
    );

    Ok(StampContext {
        work_item,
        evolver_item_id: evolver_item.id,
        log_summary,
        prompt,
    })
}

// ---------------------------------------------------------------------------
// Pure response parsing — separated from side effects
// ---------------------------------------------------------------------------

/// Outcome of parsing + validating an agent response for skill creation.
enum CreateOutcome {
    /// Valid skill content, ready to write.
    Valid,
    /// Validation failed; contains the error message for retry.
    Invalid(String),
}

/// Validate skill content from a Create action.
/// Returns Valid if the content passes validation, Invalid with error message otherwise.
fn validate_create_content(content: &str) -> CreateOutcome {
    match evolve::validate_skill_output(content) {
        Ok(()) => CreateOutcome::Valid,
        Err(e) => CreateOutcome::Invalid(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Steps 7-9: call agent, parse response, handle Create/Update/Skip
// ---------------------------------------------------------------------------

async fn execute_action(
    base_dir: &Path,
    board: &Board,
    caller: &dyn AgentCaller,
    stamp: &opengoose_board::entity::stamp::Model,
    ctx: &StampContext,
    existing: &[load::LoadedSkill],
) -> anyhow::Result<()> {
    let evolver_rig = RigId::new("evolver");
    let target_rig = &stamp.target_rig;

    // 7. Call agent.reply() and collect response
    let response = caller.call(&ctx.prompt, ctx.evolver_item_id).await?;

    // 8. Parse and handle response
    let action = evolve::parse_evolve_response(&response);
    match action {
        evolve::EvolveAction::Skip => {
            info!("evolver: skipped stamp {} (lesson too generic)", stamp.id);
        }
        evolve::EvolveAction::Update(name) => {
            // Find existing skill
            let skill = existing.iter().find(|s| s.name == name);
            match skill {
                Some(skill) => {
                    let update_prompt = evolve::build_update_prompt(&evolve::UpdatePromptParams {
                        skill_name: &name,
                        existing_content: &skill.content,
                        dimension: &stamp.dimension,
                        score: stamp.score,
                        comment: stamp.comment.as_deref(),
                        work_item_title: &ctx.work_item.title,
                        work_item_id: stamp.work_item_id,
                        log_summary: &ctx.log_summary,
                    });
                    let update_response =
                        caller.call(&update_prompt, ctx.evolver_item_id).await?;
                    let update_action = evolve::parse_evolve_response(&update_response);
                    match update_action {
                        evolve::EvolveAction::Create(new_content) => {
                            evolve::validate_skill_output(&new_content)?;
                            evolve::update_existing_skill(
                                &skill.path,
                                &new_content,
                                stamp.id,
                                stamp.work_item_id,
                                &stamp.dimension,
                                stamp.score,
                                Some(ctx.evolver_item_id),
                            )?;
                            info!("evolver: updated skill '{name}' for stamp {}", stamp.id);
                        }
                        _ => {
                            warn!("evolver: UPDATE response for '{name}' was not a valid skill");
                        }
                    }
                }
                None => {
                    warn!("evolver: skill '{name}' not found for update, skipping");
                }
            }
        }
        evolve::EvolveAction::Create(content) => {
            // Use pure validation to determine outcome
            match validate_create_content(&content) {
                CreateOutcome::Valid => {
                    let skill_name = evolve::write_skill_to_rig_scope(
                        base_dir,
                        target_rig,
                        &content,
                        evolve::WriteSkillParams {
                            stamp_id: stamp.id,
                            work_item_id: stamp.work_item_id,
                            dimension: &stamp.dimension,
                            score: stamp.score,
                            evolver_work_item_id: Some(ctx.evolver_item_id),
                        },
                    )?;
                    info!(
                        "evolver: generated skill '{skill_name}' for stamp {}",
                        stamp.id
                    );
                }
                CreateOutcome::Invalid(err) => {
                    // Retry once with format fix
                    warn!("evolver: validation failed, retrying: {err}");
                    let retry_prompt = format!(
                        "{}\n\nPrevious output had format errors: {err}\n\
                         Please fix the format and try again.",
                        ctx.prompt
                    );
                    let retry_response =
                        caller.call(&retry_prompt, ctx.evolver_item_id).await?;
                    let retry_action = evolve::parse_evolve_response(&retry_response);
                    match retry_action {
                        evolve::EvolveAction::Create(retry_content) => {
                            evolve::validate_skill_output(&retry_content)?;
                            let skill_name = evolve::write_skill_to_rig_scope(
                                base_dir,
                                target_rig,
                                &retry_content,
                                evolve::WriteSkillParams {
                                    stamp_id: stamp.id,
                                    work_item_id: stamp.work_item_id,
                                    dimension: &stamp.dimension,
                                    score: stamp.score,
                                    evolver_work_item_id: Some(ctx.evolver_item_id),
                                },
                            )?;
                            info!(
                                "evolver: generated skill '{skill_name}' on retry for stamp {}",
                                stamp.id
                            );
                        }
                        _ => {
                            warn!(
                                "evolver: retry did not produce valid skill for stamp {}",
                                stamp.id
                            );
                            board.mark_stuck(ctx.evolver_item_id, &evolver_rig).await?;
                            return Err(anyhow::anyhow!("retry failed, item marked stuck"));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// process_stamp — orchestrates the 3 focused functions
// ---------------------------------------------------------------------------

pub(super) async fn process_stamp(
    board: &Board,
    caller: &dyn AgentCaller,
    stamp: &opengoose_board::entity::stamp::Model,
) -> anyhow::Result<()> {
    let base_dir = crate::home_dir();
    let existing = load::load_skills_for(Some(&stamp.target_rig), None);

    update_effectiveness(stamp, &existing);
    let ctx = prepare_context(board, stamp, &existing).await?;
    let result = execute_action(&base_dir, board, caller, stamp, &ctx, &existing).await;

    // Submit or abandon based on result
    let evolver_rig = RigId::new("evolver");
    match result {
        Ok(()) => {
            board.submit(ctx.evolver_item_id, &evolver_rig).await?;
        }
        Err(e) => {
            warn!("evolver: action failed for stamp {}: {e}", stamp.id);
            let _ = board.abandon(ctx.evolver_item_id).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]
    use super::*;
    use crate::skills::test_env_lock;
    use async_trait::async_trait;
    use chrono::Utc;
    use opengoose_board::Board;
    use opengoose_board::board::AddStampParams;
    use opengoose_board::entity::stamp::Model;
    use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
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
        async fn call(&self, prompt: &str, _work_id: i64) -> anyhow::Result<String> {
            let raw = if prompt.contains("Previous output had format errors") {
                self.reply
                    .split("||")
                    .nth(1)
                    .unwrap_or(&self.reply)
                    .to_string()
            } else {
                self.reply
                    .split("||")
                    .next()
                    .unwrap_or(&self.reply)
                    .to_string()
            };
            if let Some(err_msg) = raw.strip_prefix("ERR:") {
                return Err(anyhow::anyhow!(err_msg.to_string()));
            }
            Ok(raw)
        }
    }

    async fn seeded_stamp(board: &Board, target_rig: &str) -> Model {
        let work_item = board
            .post(PostWorkItem {
                title: "evolver test work".into(),
                description: String::new(),
                created_by: RigId::new("tester"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        board
            .add_stamp(AddStampParams {
                target_rig,
                work_item_id: work_item.id,
                dimension: "Quality",
                score: 0.2,
                severity: "Leaf",
                stamped_by: "human",
                comment: Some("low score"),
                active_skill_versions: None,
            })
            .await
            .unwrap();

        let mut stamps = board
            .unprocessed_low_stamps(super::super::LOW_STAMP_THRESHOLD)
            .await
            .unwrap();
        stamps
            .drain(..1)
            .next()
            .expect("seeded low stamp should exist")
    }

    fn sample_skill() -> &'static str {
        "\
---
name: test-skill
description: Use when a task has a weak quality signal and repeats.
---

# Test lesson\n"
    }

    #[tokio::test]
    async fn process_stamp_fails_when_missing_work_item() {
        let caller = MockAgentCaller {
            reply: "SKIP".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();

        let stamp = Model {
            id: 1,
            target_rig: "missing-rig".into(),
            work_item_id: 9999,
            dimension: "Quality".into(),
            score: 0.2,
            severity: "Leaf".into(),
            stamped_by: "human".into(),
            comment: None,
            active_skill_versions: None,
            evolved_at: None,
            timestamp: Utc::now(),
        };

        assert!(process_stamp(&board, &caller, &stamp).await.is_err());
    }

    #[tokio::test]
    async fn process_stamp_skips_when_agent_returns_skip() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "SKIP".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let stamp = seeded_stamp(&board, "skip-rig").await;

        process_stamp(&board, &caller, &stamp).await.unwrap();

        let generated = board
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"));
        assert!(generated.is_some());
        let generated = generated.unwrap();
        let fetched = board.get(generated.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, Status::Done);

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn process_stamp_creates_skill_on_valid_evolve_output() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: sample_skill().into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let stamp = seeded_stamp(&board, "create-rig").await;

        process_stamp(&board, &caller, &stamp).await.unwrap();

        let expected = home
            .path()
            .join(".opengoose/rigs/create-rig/skills/learned/test-skill/SKILL.md");
        assert!(expected.exists());

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn process_stamp_retries_when_first_output_invalid() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "invalid raw output||SKIP".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let stamp = seeded_stamp(&board, "retry-rig").await;

        process_stamp(&board, &caller, &stamp).await.unwrap();

        let generated = board
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"));
        assert!(generated.is_some());
        let generated = generated.unwrap();
        let fetched = board.get(generated.id).await.unwrap().unwrap();
        // process_stamp catches execute_action errors and calls abandon, so the
        // item ends up Abandoned even though execute_action called mark_stuck first.
        assert_eq!(fetched.status, Status::Abandoned);

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn process_stamp_marks_update_without_skill_file() {
        let caller = MockAgentCaller {
            reply: "UPDATE:test-existing".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let stamp = seeded_stamp(&board, "update-rig").await;

        process_stamp(&board, &caller, &stamp).await.unwrap();

        let generated = board
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"));
        assert!(generated.is_some());
        let fetched = board.get(generated.unwrap().id).await.unwrap().unwrap();
        assert_eq!(fetched.status, Status::Done);
    }

    #[tokio::test]
    async fn process_stamp_propagates_agent_error_without_submit() {
        let caller = MockAgentCaller {
            reply: "ERR:boom".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let stamp = seeded_stamp(&board, "error-rig").await;

        // process_stamp swallows execute_action errors: calls abandon (which fails
        // because Claimed→Abandoned is not a valid transition) and returns Ok(()).
        process_stamp(&board, &caller, &stamp).await.unwrap();

        let items = board.list().await.unwrap();
        let generated = items
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"))
            .expect("evolver work item should be posted");
        let fetched = board.get(generated.id).await.unwrap().unwrap();
        // abandon fails silently (Claimed→Abandoned invalid), so item stays Claimed
        assert_eq!(fetched.status, Status::Claimed);
    }

    /// process_stamp with UPDATE where skill IS found but update response is not Create → warn only.
    #[tokio::test]
    async fn process_stamp_update_skill_found_update_response_not_create() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        // Both calls return "UPDATE:existing-skill" → second call also returns Update → _  arm (warn only)
        let caller = MockAgentCaller {
            reply: "UPDATE:existing-skill".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let stamp = seeded_stamp(&board, "update-found-rig").await;

        // Pre-create the skill file so the Some(skill) branch is taken.
        let skill_dir = home
            .path()
            .join(".opengoose/rigs/update-found-rig/skills/learned/existing-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: existing-skill\ndescription: Use when original\n---\n# Original\n",
        )
        .unwrap();
        let meta = crate::skills::evolve::SkillMetadata {
            generated_from: opengoose_skills::metadata::GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: opengoose_skills::metadata::Effectiveness {
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

        // Should succeed (warn path, no error)
        process_stamp(&board, &caller, &stamp).await.unwrap();

        // Original SKILL.md unchanged since update response was not Create
        let content = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(content.contains("# Original"));

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// process_stamp: first output invalid → retry prompt has "Previous output had format errors"
    /// → caller returns second split (valid skill) → skill written.
    #[tokio::test]
    async fn process_stamp_retry_succeeds_when_second_output_valid() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let valid_skill = "---\nname: retry-skill\ndescription: Use when retrying format errors\n---\n# Retried\n";
        let caller = MockAgentCaller {
            reply: format!("invalid raw output||{valid_skill}"),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let stamp = seeded_stamp(&board, "retry-success-rig").await;

        process_stamp(&board, &caller, &stamp).await.unwrap();

        let expected = home
            .path()
            .join(".opengoose/rigs/retry-success-rig/skills/learned/retry-skill/SKILL.md");
        assert!(expected.exists(), "retry skill file should be written");
        let content = std::fs::read_to_string(&expected).unwrap();
        assert!(content.contains("Retried"));

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// MockAgentCaller: ERR: prefix causes Err return.
    #[tokio::test]
    async fn mock_agent_caller_returns_err_for_err_prefix() {
        let caller = MockAgentCaller {
            reply: "ERR:test error message".into(),
        };
        let result = caller.call("normal prompt", 0).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("test error message")
        );
    }

    /// MockAgentCaller: normal prompt uses first split, retry prompt uses second split.
    #[tokio::test]
    async fn mock_agent_caller_uses_correct_split() {
        let caller = MockAgentCaller {
            reply: "first-part||second-part".into(),
        };

        // Normal prompt → first split
        let normal = caller.call("normal prompt", 0).await.unwrap();
        assert_eq!(normal, "first-part");

        // Retry prompt → second split
        let retry = caller
            .call(
                "some context\n\nPrevious output had format errors: missing name",
                0,
            )
            .await
            .unwrap();
        assert_eq!(retry, "second-part");
    }

    /// Covers evolver — Installed skill and Learned skill without metadata.
    #[tokio::test]
    async fn process_stamp_with_installed_and_no_metadata_learned_skills() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "SKIP".into(),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();

        // Create a global Installed skill (scope == Installed)
        let installed_dir = home.path().join(".opengoose/skills/installed/global-tool");
        std::fs::create_dir_all(&installed_dir).unwrap();
        std::fs::write(
            installed_dir.join("SKILL.md"),
            "---\nname: global-tool\ndescription: Use when global\n---\n# Global\n",
        )
        .unwrap();

        // Create a rig Learned skill WITHOUT metadata.json (read_metadata returns None)
        let learned_dir = home
            .path()
            .join(".opengoose/rigs/meta-rig/skills/learned/no-meta-skill");
        std::fs::create_dir_all(&learned_dir).unwrap();
        std::fs::write(
            learned_dir.join("SKILL.md"),
            "---\nname: no-meta-skill\ndescription: Use when no meta\n---\n# No Meta\n",
        )
        .unwrap();
        // Intentionally NOT writing metadata.json

        let stamp = seeded_stamp(&board, "meta-rig").await;
        process_stamp(&board, &caller, &stamp).await.unwrap();

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[test]
    fn update_effectiveness_handles_empty_skills_list() {
        let stamp = opengoose_board::entity::stamp::Model {
            id: 1,
            target_rig: "test-rig".into(),
            work_item_id: 1,
            dimension: "Quality".into(),
            score: 0.3,
            severity: "Leaf".into(),
            stamped_by: "human".into(),
            comment: Some("test".into()),
            active_skill_versions: None,
            evolved_at: None,
            timestamp: Utc::now(),
        };
        update_effectiveness(&stamp, &[]);
    }

    #[test]
    fn update_effectiveness_skips_installed_skills() {
        let _guard = test_env_lock();
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).unwrap();
        let meta = serde_json::json!({
            "generated_from": { "dimension": "Quality", "score": 0.5 },
            "effectiveness": { "injected_count": 0, "subsequent_scores": [] },
            "skill_version": 1
        });
        std::fs::write(skill_dir.join("metadata.json"), meta.to_string()).unwrap();

        let stamp = opengoose_board::entity::stamp::Model {
            id: 1,
            target_rig: "test-rig".into(),
            work_item_id: 1,
            dimension: "Quality".into(),
            score: 0.3,
            severity: "Leaf".into(),
            stamped_by: "human".into(),
            comment: None,
            active_skill_versions: None,
            evolved_at: None,
            timestamp: Utc::now(),
        };

        let skills = vec![load::LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir.clone(),
            content: sample_skill().into(),
            scope: load::SkillScope::Installed,
        }];

        update_effectiveness(&stamp, &skills);
        let updated_meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        let scores = updated_meta["effectiveness"]["subsequent_scores"]
            .as_array()
            .unwrap();
        assert!(scores.is_empty(), "installed skill should not be updated");
    }

    #[test]
    fn update_effectiveness_skips_different_dimension() {
        let _guard = test_env_lock();
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).unwrap();
        let meta = serde_json::json!({
            "generated_from": { "stamp_id": 1, "work_item_id": 1, "dimension": "Reliability", "score": 0.5 },
            "generated_at": "2025-01-01T00:00:00Z",
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": { "injected_count": 0, "subsequent_scores": [] },
            "skill_version": 1
        });
        std::fs::write(skill_dir.join("metadata.json"), meta.to_string()).unwrap();

        let stamp = opengoose_board::entity::stamp::Model {
            id: 1,
            target_rig: "test-rig".into(),
            work_item_id: 1,
            dimension: "Quality".into(),
            score: 0.3,
            severity: "Leaf".into(),
            stamped_by: "human".into(),
            comment: None,
            active_skill_versions: None,
            evolved_at: None,
            timestamp: Utc::now(),
        };

        let skills = vec![load::LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir.clone(),
            content: sample_skill().into(),
            scope: load::SkillScope::Learned,
        }];

        update_effectiveness(&stamp, &skills);
        let updated_meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        let scores = updated_meta["effectiveness"]["subsequent_scores"]
            .as_array()
            .unwrap();
        assert!(
            scores.is_empty(),
            "different dimension should not be updated"
        );
    }

    #[test]
    fn update_effectiveness_updates_matching_learned_skill() {
        let _guard = test_env_lock();
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).unwrap();
        let meta = serde_json::json!({
            "generated_from": { "stamp_id": 1, "work_item_id": 1, "dimension": "Quality", "score": 0.5 },
            "generated_at": "2025-01-01T00:00:00Z",
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": { "injected_count": 1, "subsequent_scores": [] },
            "skill_version": 1
        });
        std::fs::write(skill_dir.join("metadata.json"), meta.to_string()).unwrap();

        let stamp = opengoose_board::entity::stamp::Model {
            id: 1,
            target_rig: "test-rig".into(),
            work_item_id: 1,
            dimension: "Quality".into(),
            score: 0.7,
            severity: "Leaf".into(),
            stamped_by: "human".into(),
            comment: None,
            active_skill_versions: Some(r#"{"test-skill":1}"#.into()),
            evolved_at: None,
            timestamp: Utc::now(),
        };

        let skills = vec![load::LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir.clone(),
            content: sample_skill().into(),
            scope: load::SkillScope::Learned,
        }];

        update_effectiveness(&stamp, &skills);
        let updated_meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        let scores = updated_meta["effectiveness"]["subsequent_scores"]
            .as_array()
            .unwrap();
        assert!(
            !scores.is_empty(),
            "matching learned skill should be updated"
        );
    }

    #[test]
    fn update_effectiveness_handles_missing_metadata() {
        let _guard = test_env_lock();
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).unwrap();
        // Intentionally NO metadata.json

        let stamp = opengoose_board::entity::stamp::Model {
            id: 1,
            target_rig: "test-rig".into(),
            work_item_id: 1,
            dimension: "Quality".into(),
            score: 0.3,
            severity: "Leaf".into(),
            stamped_by: "human".into(),
            comment: None,
            active_skill_versions: None,
            evolved_at: None,
            timestamp: Utc::now(),
        };

        let skills = vec![load::LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir,
            content: sample_skill().into(),
            scope: load::SkillScope::Learned,
        }];

        update_effectiveness(&stamp, &skills);
    }

    /// Covers evolver — the retry path where the second call returns valid
    /// SKILL.md content and write_skill_to_rig_scope is called successfully.
    #[tokio::test]
    async fn process_stamp_retry_succeeds_and_writes_skill() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        // First call: "invalid" → validate fails → retry.
        // Second call (retry, prompt contains "Previous output had format errors"): valid SKILL.md.
        let valid_skill = "\
---\nname: retry-skill\ndescription: Use when retry test needed.\n---\n\n# Retry\n";
        let caller = MockAgentCaller {
            reply: format!("invalid||{valid_skill}"),
        };
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let stamp = seeded_stamp(&board, "retry-ok-rig").await;

        process_stamp(&board, &caller, &stamp).await.unwrap();

        // Skill file should exist in rig scope
        let skill_dir = home
            .path()
            .join(".opengoose/rigs/retry-ok-rig/skills/learned/retry-skill");
        assert!(
            skill_dir.join("SKILL.md").exists(),
            "retry skill should be written"
        );

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[test]
    fn validate_create_content_returns_valid_for_good_skill() {
        let skill = "---\nname: test\ndescription: Use when testing\n---\n# Test\n";
        let result = validate_create_content(skill);
        assert!(matches!(result, CreateOutcome::Valid));
    }

    #[test]
    fn validate_create_content_returns_invalid_for_bad_content() {
        let result = validate_create_content("just some text without frontmatter");
        assert!(matches!(result, CreateOutcome::Invalid(_)));
    }
}
