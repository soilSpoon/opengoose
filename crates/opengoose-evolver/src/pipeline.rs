// Stamp processing pipeline — prepare_context, execute_action, process_stamp.

use crate::AgentCaller;
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
use opengoose_skills::evolution::parser::{EvolveAction, parse_evolve_response};
use opengoose_skills::evolution::prompts::{
    UpdatePromptParams, build_evolve_prompt, build_update_prompt,
};
use opengoose_skills::evolution::validator::validate_skill_output;
use opengoose_skills::evolution::writer::{
    WriteSkillParams, update_effectiveness_versioned, update_existing_skill,
    write_skill_to_rig_scope,
};
use opengoose_skills::loader::{LoadedSkill, SkillScope, load_skills};
use opengoose_skills::metadata::read_metadata;
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

fn update_effectiveness(stamp: &opengoose_board::entity::stamp::Model, existing: &[LoadedSkill]) {
    for skill in existing {
        if should_update_effectiveness(skill, &stamp.dimension)
            && let Err(e) = update_effectiveness_versioned(
                &skill.path,
                stamp.score,
                stamp.active_skill_versions.as_deref(),
            )
        {
            warn!(
                "evolver: effectiveness update failed for '{}': {e}",
                skill.name
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Build (name, description) pairs from loaded skills for dedup checking.
fn build_existing_skill_pairs(existing: &[LoadedSkill]) -> Vec<(String, String)> {
    existing
        .iter()
        .map(|s| (s.name.clone(), s.description.clone()))
        .collect()
}

/// Result of pure prompt construction — no I/O involved.
pub(crate) struct PreparedPrompt {
    pub prompt: String,
}

/// Build the evolve prompt from stamp data and loaded skills, with no I/O.
pub(crate) fn build_evolve_prompt_pure(
    dimension: &str,
    score: f32,
    comment: Option<&str>,
    work_item_title: &str,
    work_item_id: i64,
    log_summary: &str,
    skills: &[LoadedSkill],
) -> PreparedPrompt {
    let existing_skill_pairs = build_existing_skill_pairs(skills);
    let prompt = build_evolve_prompt(
        dimension,
        score,
        comment,
        work_item_title,
        work_item_id,
        log_summary,
        &existing_skill_pairs,
    );
    PreparedPrompt { prompt }
}

/// Parsed outcome of an LLM response — pure, no side effects.
#[derive(Debug, PartialEq)]
pub(crate) enum ParsedAction {
    Skip,
    Update(String),
    Create(String),
}

/// Parse and validate an LLM response into a `ParsedAction`.
/// Returns `Err` only if the response is completely unparseable (currently
/// `parse_evolve_response` always succeeds, so this is future-proofing).
pub(crate) fn validate_and_parse_response(raw: &str) -> anyhow::Result<ParsedAction> {
    match parse_evolve_response(raw) {
        EvolveAction::Skip => Ok(ParsedAction::Skip),
        EvolveAction::Update(name) => Ok(ParsedAction::Update(name)),
        EvolveAction::Create(content) => Ok(ParsedAction::Create(content)),
    }
}

/// Check whether a learned skill's dimension matches the stamp's dimension,
/// indicating its effectiveness score should be updated.
fn should_update_effectiveness(skill: &LoadedSkill, stamp_dimension: &str) -> bool {
    skill.scope == SkillScope::Learned
        && read_metadata(&skill.path)
            .is_some_and(|meta| meta.generated_from.dimension == stamp_dimension)
}

// ---------------------------------------------------------------------------
// Steps 1-6: load work item, post+claim evolver item, read log, build prompt
// ---------------------------------------------------------------------------

async fn prepare_context(
    board: &Board,
    stamp: &opengoose_board::entity::stamp::Model,
    existing: &[LoadedSkill],
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
    let log_summary = crate::read_conversation_log(stamp.work_item_id);

    // 5-6. Build prompt (pure computation)
    let prepared = build_evolve_prompt_pure(
        &stamp.dimension,
        stamp.score,
        stamp.comment.as_deref(),
        &work_item.title,
        stamp.work_item_id,
        &log_summary,
        existing,
    );

    Ok(StampContext {
        work_item,
        evolver_item_id: evolver_item.id,
        log_summary,
        prompt: prepared.prompt,
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
    match validate_skill_output(content) {
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
    existing: &[LoadedSkill],
) -> anyhow::Result<()> {
    let evolver_rig = RigId::new("evolver");
    let target_rig = &stamp.target_rig;

    // 7. Call agent.reply() and collect response
    let response = caller.call(&ctx.prompt, ctx.evolver_item_id).await?;

    // 8. Parse and handle response
    let action = validate_and_parse_response(&response)?;
    match action {
        ParsedAction::Skip => {
            info!("evolver: skipped stamp {} (lesson too generic)", stamp.id);
        }
        ParsedAction::Update(name) => {
            // Find existing skill
            let skill = existing.iter().find(|s| s.name == name);
            match skill {
                Some(skill) => {
                    let update_prompt = build_update_prompt(&UpdatePromptParams {
                        skill_name: &name,
                        existing_content: &skill.content,
                        dimension: &stamp.dimension,
                        score: stamp.score,
                        comment: stamp.comment.as_deref(),
                        work_item_title: &ctx.work_item.title,
                        work_item_id: stamp.work_item_id,
                        log_summary: &ctx.log_summary,
                    });
                    let update_response = caller.call(&update_prompt, ctx.evolver_item_id).await?;
                    let update_action = validate_and_parse_response(&update_response)?;
                    match update_action {
                        ParsedAction::Create(new_content) => {
                            validate_skill_output(&new_content)?;
                            update_existing_skill(
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
        ParsedAction::Create(content) => {
            // Use pure validation to determine outcome
            match validate_create_content(&content) {
                CreateOutcome::Valid => {
                    let skill_name = write_skill_to_rig_scope(
                        base_dir,
                        target_rig,
                        &content,
                        WriteSkillParams {
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
                    let retry_response = caller.call(&retry_prompt, ctx.evolver_item_id).await?;
                    let retry_action = validate_and_parse_response(&retry_response)?;
                    match retry_action {
                        ParsedAction::Create(retry_content) => {
                            validate_skill_output(&retry_content)?;
                            let skill_name = write_skill_to_rig_scope(
                                base_dir,
                                target_rig,
                                &retry_content,
                                WriteSkillParams {
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

pub(crate) async fn process_stamp(
    board: &Board,
    caller: &dyn AgentCaller,
    stamp: &opengoose_board::entity::stamp::Model,
) -> anyhow::Result<()> {
    let base_dir = crate::home_dir();
    let existing = load_skills(&base_dir, Some(&stamp.target_rig), None);

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
            if let Err(stuck_err) = board.mark_stuck(ctx.evolver_item_id, &evolver_rig).await {
                warn!(
                    "evolver: failed to mark item {} stuck: {stuck_err}",
                    ctx.evolver_item_id
                );
            }
            return Err(e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]
    use super::*;
    use crate::test_env_lock;
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
    impl crate::AgentCaller for MockAgentCaller {
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
            .expect("should post work item");

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
            .expect("should add stamp");

        let mut stamps = board
            .unprocessed_low_stamps(crate::LOW_STAMP_THRESHOLD)
            .await
            .expect("should query unprocessed low stamps");
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
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");

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
        let home = tempdir().expect("should create temp dir");
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "SKIP".into(),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");
        let stamp = seeded_stamp(&board, "skip-rig").await;

        process_stamp(&board, &caller, &stamp)
            .await
            .expect("process_stamp should succeed");

        let generated = board
            .list()
            .await
            .expect("should list work items")
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"));
        assert!(generated.is_some());
        let generated = generated.expect("should find generated work item");
        let fetched = board
            .get(generated.id)
            .await
            .expect("should get work item")
            .expect("work item should exist");
        assert_eq!(fetched.status, Status::Done);

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn process_stamp_creates_skill_on_valid_evolve_output() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().expect("should create temp dir");
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: sample_skill().into(),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");
        let stamp = seeded_stamp(&board, "create-rig").await;

        process_stamp(&board, &caller, &stamp)
            .await
            .expect("process_stamp should succeed");

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
        let home = tempdir().expect("should create temp dir");
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "invalid raw output||SKIP".into(),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");
        let stamp = seeded_stamp(&board, "retry-rig").await;

        let result = process_stamp(&board, &caller, &stamp).await;
        assert!(
            result.is_err(),
            "process_stamp should propagate retry failure"
        );

        let generated = board
            .list()
            .await
            .expect("should list work items")
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"));
        assert!(generated.is_some());
        let generated = generated.expect("should find generated work item");
        let fetched = board
            .get(generated.id)
            .await
            .expect("should get work item")
            .expect("work item should exist");
        // execute_action marks item stuck on retry failure, process_stamp propagates the error
        assert_eq!(fetched.status, Status::Stuck);

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    #[tokio::test]
    async fn process_stamp_marks_update_without_skill_file() {
        let caller = MockAgentCaller {
            reply: "UPDATE:test-existing".into(),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");
        let stamp = seeded_stamp(&board, "update-rig").await;

        process_stamp(&board, &caller, &stamp)
            .await
            .expect("process_stamp should succeed");

        let generated = board
            .list()
            .await
            .expect("should list work items")
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"));
        assert!(generated.is_some());
        let fetched = board
            .get(generated.expect("should find generated item").id)
            .await
            .expect("should get work item")
            .expect("work item should exist");
        assert_eq!(fetched.status, Status::Done);
    }

    #[tokio::test]
    async fn process_stamp_propagates_agent_error_without_submit() {
        let caller = MockAgentCaller {
            reply: "ERR:boom".into(),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");
        let stamp = seeded_stamp(&board, "error-rig").await;

        // process_stamp now propagates execute_action errors and marks the item stuck.
        let result = process_stamp(&board, &caller, &stamp).await;
        assert!(result.is_err(), "process_stamp should propagate the error");

        let items = board.list().await.expect("should list work items");
        let generated = items
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"))
            .expect("evolver work item should be posted");
        let fetched = board
            .get(generated.id)
            .await
            .expect("should get work item")
            .expect("work item should exist");
        assert_eq!(fetched.status, Status::Stuck);
    }

    /// process_stamp with UPDATE where skill IS found but update response is not Create -> warn only.
    #[tokio::test]
    async fn process_stamp_update_skill_found_update_response_not_create() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().expect("should create temp dir");
        let prev_home = set_env_var("HOME", home.path().to_str());

        // Both calls return "UPDATE:existing-skill" -> second call also returns Update -> _ arm (warn only)
        let caller = MockAgentCaller {
            reply: "UPDATE:existing-skill".into(),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");
        let stamp = seeded_stamp(&board, "update-found-rig").await;

        // Pre-create the skill file so the Some(skill) branch is taken.
        let skill_dir = home
            .path()
            .join(".opengoose/rigs/update-found-rig/skills/learned/existing-skill");
        std::fs::create_dir_all(&skill_dir).expect("should create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: existing-skill\ndescription: Use when original\n---\n# Original\n",
        )
        .expect("should write SKILL.md");
        let meta = opengoose_skills::metadata::SkillMetadata {
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
            serde_json::to_string_pretty(&meta).expect("should serialize metadata"),
        )
        .expect("should write metadata.json");

        // Should succeed (warn path, no error)
        process_stamp(&board, &caller, &stamp)
            .await
            .expect("process_stamp should succeed");

        // Original SKILL.md unchanged since update response was not Create
        let content =
            std::fs::read_to_string(skill_dir.join("SKILL.md")).expect("should read SKILL.md");
        assert!(content.contains("# Original"));

        restore_env_var("HOME", prev_home);
        drop(guard);
    }

    /// process_stamp: first output invalid -> retry prompt has "Previous output had format errors"
    /// -> caller returns second split (valid skill) -> skill written.
    #[tokio::test]
    async fn process_stamp_retry_succeeds_when_second_output_valid() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().expect("should create temp dir");
        let prev_home = set_env_var("HOME", home.path().to_str());

        let valid_skill = "---\nname: retry-skill\ndescription: Use when retrying format errors\n---\n# Retried\n";
        let caller = MockAgentCaller {
            reply: format!("invalid raw output||{valid_skill}"),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");
        let stamp = seeded_stamp(&board, "retry-success-rig").await;

        process_stamp(&board, &caller, &stamp)
            .await
            .expect("process_stamp should succeed");

        let expected = home
            .path()
            .join(".opengoose/rigs/retry-success-rig/skills/learned/retry-skill/SKILL.md");
        assert!(expected.exists(), "retry skill file should be written");
        let content = std::fs::read_to_string(&expected).expect("should read retry skill file");
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
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("test error message"),
            "expected error containing 'test error message', got: {err}"
        );
    }

    /// MockAgentCaller: normal prompt uses first split, retry prompt uses second split.
    #[tokio::test]
    async fn mock_agent_caller_uses_correct_split() {
        let caller = MockAgentCaller {
            reply: "first-part||second-part".into(),
        };

        // Normal prompt -> first split
        let normal = caller
            .call("normal prompt", 0)
            .await
            .expect("should succeed for normal prompt");
        assert_eq!(normal, "first-part");

        // Retry prompt -> second split
        let retry = caller
            .call(
                "some context\n\nPrevious output had format errors: missing name",
                0,
            )
            .await
            .expect("should succeed for retry prompt");
        assert_eq!(retry, "second-part");
    }

    /// Covers evolver — Installed skill and Learned skill without metadata.
    #[tokio::test]
    async fn process_stamp_with_installed_and_no_metadata_learned_skills() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().expect("should create temp dir");
        let prev_home = set_env_var("HOME", home.path().to_str());

        let caller = MockAgentCaller {
            reply: "SKIP".into(),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");

        // Create a global Installed skill (scope == Installed)
        let installed_dir = home.path().join(".opengoose/skills/installed/global-tool");
        std::fs::create_dir_all(&installed_dir).expect("should create installed skill dir");
        std::fs::write(
            installed_dir.join("SKILL.md"),
            "---\nname: global-tool\ndescription: Use when global\n---\n# Global\n",
        )
        .expect("should write installed SKILL.md");

        // Create a rig Learned skill WITHOUT metadata.json (read_metadata returns None)
        let learned_dir = home
            .path()
            .join(".opengoose/rigs/meta-rig/skills/learned/no-meta-skill");
        std::fs::create_dir_all(&learned_dir).expect("should create learned skill dir");
        std::fs::write(
            learned_dir.join("SKILL.md"),
            "---\nname: no-meta-skill\ndescription: Use when no meta\n---\n# No Meta\n",
        )
        .expect("should write learned SKILL.md");
        // Intentionally NOT writing metadata.json

        let stamp = seeded_stamp(&board, "meta-rig").await;
        process_stamp(&board, &caller, &stamp)
            .await
            .expect("process_stamp should succeed");

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
        let dir = tempdir().expect("should create temp dir");
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).expect("should create skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).expect("should write SKILL.md");
        let meta = serde_json::json!({
            "generated_from": { "dimension": "Quality", "score": 0.5 },
            "effectiveness": { "injected_count": 0, "subsequent_scores": [] },
            "skill_version": 1
        });
        std::fs::write(skill_dir.join("metadata.json"), meta.to_string())
            .expect("should write metadata.json");

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

        let skills = vec![LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir.clone(),
            content: sample_skill().into(),
            scope: SkillScope::Installed,
        }];

        update_effectiveness(&stamp, &skills);
        let updated_meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("should read metadata.json"),
        )
        .expect("should parse metadata JSON");
        let scores = updated_meta["effectiveness"]["subsequent_scores"]
            .as_array()
            .expect("subsequent_scores should be an array");
        assert!(scores.is_empty(), "installed skill should not be updated");
    }

    #[test]
    fn update_effectiveness_skips_different_dimension() {
        let _guard = test_env_lock();
        let dir = tempdir().expect("should create temp dir");
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).expect("should create skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).expect("should write SKILL.md");
        let meta = serde_json::json!({
            "generated_from": { "stamp_id": 1, "work_item_id": 1, "dimension": "Reliability", "score": 0.5 },
            "generated_at": "2025-01-01T00:00:00Z",
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": { "injected_count": 0, "subsequent_scores": [] },
            "skill_version": 1
        });
        std::fs::write(skill_dir.join("metadata.json"), meta.to_string())
            .expect("should write metadata.json");

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

        let skills = vec![LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir.clone(),
            content: sample_skill().into(),
            scope: SkillScope::Learned,
        }];

        update_effectiveness(&stamp, &skills);
        let updated_meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("should read metadata.json"),
        )
        .expect("should parse metadata JSON");
        let scores = updated_meta["effectiveness"]["subsequent_scores"]
            .as_array()
            .expect("subsequent_scores should be an array");
        assert!(
            scores.is_empty(),
            "different dimension should not be updated"
        );
    }

    #[test]
    fn update_effectiveness_updates_matching_learned_skill() {
        let _guard = test_env_lock();
        let dir = tempdir().expect("should create temp dir");
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).expect("should create skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).expect("should write SKILL.md");
        let meta = serde_json::json!({
            "generated_from": { "stamp_id": 1, "work_item_id": 1, "dimension": "Quality", "score": 0.5 },
            "generated_at": "2025-01-01T00:00:00Z",
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": { "injected_count": 1, "subsequent_scores": [] },
            "skill_version": 1
        });
        std::fs::write(skill_dir.join("metadata.json"), meta.to_string())
            .expect("should write metadata.json");

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

        let skills = vec![LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir.clone(),
            content: sample_skill().into(),
            scope: SkillScope::Learned,
        }];

        update_effectiveness(&stamp, &skills);
        let updated_meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("should read metadata.json"),
        )
        .expect("should parse metadata JSON");
        let scores = updated_meta["effectiveness"]["subsequent_scores"]
            .as_array()
            .expect("subsequent_scores should be an array");
        assert!(
            !scores.is_empty(),
            "matching learned skill should be updated"
        );
    }

    #[test]
    fn update_effectiveness_handles_missing_metadata() {
        let _guard = test_env_lock();
        let dir = tempdir().expect("should create temp dir");
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).expect("should create skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).expect("should write SKILL.md");
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

        let skills = vec![LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir,
            content: sample_skill().into(),
            scope: SkillScope::Learned,
        }];

        update_effectiveness(&stamp, &skills);
    }

    /// Covers evolver — the retry path where the second call returns valid
    /// SKILL.md content and write_skill_to_rig_scope is called successfully.
    #[tokio::test]
    async fn process_stamp_retry_succeeds_and_writes_skill() {
        let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().expect("should create temp dir");
        let prev_home = set_env_var("HOME", home.path().to_str());

        // First call: "invalid" -> validate fails -> retry.
        // Second call (retry, prompt contains "Previous output had format errors"): valid SKILL.md.
        let valid_skill = "\
---\nname: retry-skill\ndescription: Use when retry test needed.\n---\n\n# Retry\n";
        let caller = MockAgentCaller {
            reply: format!("invalid||{valid_skill}"),
        };
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("should connect to in-memory db");
        let stamp = seeded_stamp(&board, "retry-ok-rig").await;

        process_stamp(&board, &caller, &stamp)
            .await
            .expect("process_stamp should succeed");

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

    // -----------------------------------------------------------------------
    // build_existing_skill_pairs tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_existing_skill_pairs_maps_name_description() {
        let skills = vec![
            LoadedSkill {
                name: "alpha".into(),
                description: "Alpha skill".into(),
                path: std::path::PathBuf::from("/tmp/alpha"),
                content: "# Alpha".into(),
                scope: SkillScope::Learned,
            },
            LoadedSkill {
                name: "beta".into(),
                description: "Beta skill".into(),
                path: std::path::PathBuf::from("/tmp/beta"),
                content: "# Beta".into(),
                scope: SkillScope::Installed,
            },
        ];
        let pairs = build_existing_skill_pairs(&skills);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("alpha".into(), "Alpha skill".into()));
        assert_eq!(pairs[1], ("beta".into(), "Beta skill".into()));
    }

    #[test]
    fn build_existing_skill_pairs_empty() {
        let pairs = build_existing_skill_pairs(&[]);
        assert!(pairs.is_empty());
    }

    // -----------------------------------------------------------------------
    // should_update_effectiveness tests
    // -----------------------------------------------------------------------

    #[test]
    fn should_update_effectiveness_matches_learned_with_same_dimension() {
        let dir = tempdir().expect("should create temp dir");
        let skill_dir = dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).expect("should create skill dir");
        std::fs::write(skill_dir.join("SKILL.md"), sample_skill()).expect("should write SKILL.md");
        let meta = serde_json::json!({
            "generated_from": { "stamp_id": 1, "work_item_id": 1, "dimension": "Quality", "score": 0.5 },
            "generated_at": "2025-01-01T00:00:00Z",
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": { "injected_count": 0, "subsequent_scores": [] },
            "skill_version": 1
        });
        std::fs::write(skill_dir.join("metadata.json"), meta.to_string())
            .expect("should write metadata.json");

        let skill = LoadedSkill {
            name: "test-skill".into(),
            description: "test".into(),
            path: skill_dir,
            content: sample_skill().into(),
            scope: SkillScope::Learned,
        };
        assert!(should_update_effectiveness(&skill, "Quality"));
    }

    #[test]
    fn should_update_effectiveness_no_metadata_returns_false() {
        let dir = tempdir().expect("should create temp dir");
        let skill_dir = dir.path().join("no-meta");
        std::fs::create_dir_all(&skill_dir).expect("should create skill dir");
        // No metadata.json

        let skill = LoadedSkill {
            name: "no-meta".into(),
            description: "test".into(),
            path: skill_dir,
            content: "# test".into(),
            scope: SkillScope::Learned,
        };
        assert!(!should_update_effectiveness(&skill, "Quality"));
    }

    #[test]
    fn should_update_effectiveness_installed_scope_returns_false() {
        let skill = LoadedSkill {
            name: "installed".into(),
            description: "test".into(),
            path: std::path::PathBuf::from("/tmp/installed"),
            content: "# test".into(),
            scope: SkillScope::Installed,
        };
        assert!(!should_update_effectiveness(&skill, "Quality"));
    }

    // -----------------------------------------------------------------------
    // build_evolve_prompt_pure tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_evolve_prompt_pure_with_empty_skills() {
        let result = build_evolve_prompt_pure(
            "Quality",
            0.2,
            Some("low score"),
            "Fix the widget",
            42,
            "user asked to fix widget",
            &[],
        );
        assert!(result.prompt.contains("Quality"));
        assert!(result.prompt.contains("Fix the widget"));
        assert!(result.prompt.contains("#42"));
        assert!(result.prompt.contains("low score"));
        assert!(!result.prompt.contains("Existing Skills"));
    }

    #[test]
    fn build_evolve_prompt_pure_with_skills_present() {
        let skills = vec![
            LoadedSkill {
                name: "alpha-skill".into(),
                description: "Alpha description".into(),
                path: std::path::PathBuf::from("/tmp/alpha"),
                content: "# Alpha".into(),
                scope: SkillScope::Learned,
            },
            LoadedSkill {
                name: "beta-skill".into(),
                description: "Beta description".into(),
                path: std::path::PathBuf::from("/tmp/beta"),
                content: "# Beta".into(),
                scope: SkillScope::Installed,
            },
        ];
        let result = build_evolve_prompt_pure(
            "Reliability",
            0.3,
            None,
            "Deploy service",
            99,
            "deployment failed",
            &skills,
        );
        assert!(result.prompt.contains("alpha-skill"));
        assert!(result.prompt.contains("beta-skill"));
        assert!(result.prompt.contains("Existing Skills"));
    }

    #[test]
    fn build_evolve_prompt_pure_with_empty_log_summary() {
        let result = build_evolve_prompt_pure("Quality", 0.5, None, "Some task", 1, "", &[]);
        assert!(!result.prompt.contains("Conversation Log"));
        assert!(result.prompt.contains("Quality"));
    }

    // -----------------------------------------------------------------------
    // validate_and_parse_response tests
    // -----------------------------------------------------------------------

    #[test]
    fn validate_and_parse_response_skip() {
        let result = validate_and_parse_response("SKIP").expect("should parse");
        assert_eq!(result, ParsedAction::Skip);
    }

    #[test]
    fn validate_and_parse_response_skip_with_whitespace() {
        let result = validate_and_parse_response("  SKIP  ").expect("should parse");
        assert_eq!(result, ParsedAction::Skip);
    }

    #[test]
    fn validate_and_parse_response_update() {
        let result = validate_and_parse_response("UPDATE:my-skill").expect("should parse");
        assert_eq!(result, ParsedAction::Update("my-skill".into()));
    }

    #[test]
    fn validate_and_parse_response_create() {
        let content = "---\nname: test\ndescription: Use when testing\n---\n# Test\n";
        let result = validate_and_parse_response(content).expect("should parse");
        // parse_evolve_response trims the input, so trailing newline is stripped
        assert_eq!(result, ParsedAction::Create(content.trim().to_string()));
    }

    #[test]
    fn validate_and_parse_response_invalid_content_still_parses_as_create() {
        // parse_evolve_response treats anything that isn't SKIP/UPDATE: as Create
        let result = validate_and_parse_response("random garbage").expect("should parse");
        assert_eq!(result, ParsedAction::Create("random garbage".into()));
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn validate_create_content_never_panics(input in "\\PC*") {
                let _ = validate_create_content(&input);
            }
        }
    }

    #[test]
    fn should_update_effectiveness_different_dimension_returns_false() {
        let dir = tempdir().expect("should create temp dir");
        let skill_dir = dir.path().join("diff-dim");
        std::fs::create_dir_all(&skill_dir).expect("should create skill dir");
        let meta = serde_json::json!({
            "generated_from": { "stamp_id": 1, "work_item_id": 1, "dimension": "Reliability", "score": 0.5 },
            "generated_at": "2025-01-01T00:00:00Z",
            "evolver_work_item_id": null,
            "last_included_at": null,
            "effectiveness": { "injected_count": 0, "subsequent_scores": [] },
            "skill_version": 1
        });
        std::fs::write(skill_dir.join("metadata.json"), meta.to_string())
            .expect("should write metadata.json");

        let skill = LoadedSkill {
            name: "diff-dim".into(),
            description: "test".into(),
            path: skill_dir,
            content: "# test".into(),
            scope: SkillScope::Learned,
        };
        assert!(!should_update_effectiveness(&skill, "Quality"));
    }
}
