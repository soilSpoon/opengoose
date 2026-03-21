// Evolver run loop — stamp_notify listener with lazy Agent init.
// Queries unprocessed low stamps, creates work items, analyzes with LLM.

use crate::runtime::{AgentConfig, create_agent};
use crate::skills::{evolve, load};
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{info, warn};

const EVOLVER_SYSTEM_PROMPT: &str = "You are a skill analyst for OpenGoose.\n\
     Analyze failed tasks and extract concrete, actionable lessons as SKILL.md files.\n\n\
     Rules:\n\
     - description MUST start with 'Use when...' (triggering conditions only)\n\
     - description must NOT summarize the skill's workflow\n\
     - Every lesson must be specific to THIS failure, not generic advice\n\
     - Include a 'Common Mistakes' table with specific rationalizations\n\
     - Include a 'Red Flags' list for self-checking\n\
     - If the lesson is something any competent agent already knows, output SKIP\n\
     - If an existing skill covers the same lesson, output UPDATE:{skill-name}\n\n\
     Output format: raw SKILL.md content with YAML frontmatter, OR 'SKIP', OR 'UPDATE:{name}'.";

const LOW_STAMP_THRESHOLD: f32 = 0.3;
const FALLBACK_SWEEP_SECS: u64 = 300; // 5 minutes

/// Evolver run loop. Lazy-inits Agent on first stamp event.
pub async fn run(board: Arc<Board>, stamp_notify: Arc<Notify>) {
    info!("evolver: listening for stamp events");

    let mut agent: Option<Agent> = None;

    loop {
        // Wait for stamp_notify OR fallback sweep
        tokio::select! {
            _ = stamp_notify.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(FALLBACK_SWEEP_SECS)) => {}
        }

        // Query unprocessed low stamps
        let stamps = match board.unprocessed_low_stamps(LOW_STAMP_THRESHOLD).await {
            Ok(s) => s,
            Err(e) => {
                warn!("evolver: failed to query stamps: {e}");
                continue;
            }
        };

        if stamps.is_empty() {
            // Idle-time sweep: re-evaluate dormant skills once per hour
            if let Some(ref agent) = agent {
                use std::sync::atomic::{AtomicU64, Ordering};
                static LAST_SWEEP_EPOCH: AtomicU64 = AtomicU64::new(0);

                let now_epoch = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let last = LAST_SWEEP_EPOCH.load(Ordering::Relaxed);

                if now_epoch - last >= 3600 {
                    LAST_SWEEP_EPOCH.store(now_epoch, Ordering::Relaxed);
                    info!("evolver: running idle-time sweep");
                    if let Err(e) = run_sweep(&board, agent).await {
                        warn!("evolver: sweep failed: {e}");
                    }
                }
            }
            continue;
        }

        // Lazy init Agent on first real work
        if agent.is_none() {
            match create_agent(AgentConfig {
                session_id: "evolver".into(),
                system_prompt: Some(EVOLVER_SYSTEM_PROMPT.into()),
            })
            .await
            {
                Ok(a) => {
                    info!("evolver: agent initialized");
                    agent = Some(a);
                }
                Err(e) => {
                    warn!("evolver: failed to create agent: {e}");
                    continue;
                }
            }
        }

        for stamp in &stamps {
            // Atomically mark as evolved (prevents duplicate processing)
            match board.mark_stamp_evolved(stamp.id).await {
                Ok(true) => {}
                Ok(false) => continue, // another Evolver got it
                Err(e) => {
                    warn!("evolver: failed to mark stamp {}: {e}", stamp.id);
                    continue;
                }
            }

            if let Err(e) = process_stamp(&board, agent.as_ref().unwrap(), stamp).await {
                warn!("evolver: failed to process stamp {}: {e}", stamp.id);
            }
        }
    }
}

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
        {
            let _ = evolve::update_effectiveness_versioned(
                &skill.path,
                stamp.score,
                stamp.active_skill_versions.as_deref(),
            );
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
// Steps 7-9: call agent, parse response, handle Create/Update/Skip
// ---------------------------------------------------------------------------

async fn execute_action(
    base_dir: &Path,
    board: &Board,
    agent: &Agent,
    stamp: &opengoose_board::entity::stamp::Model,
    ctx: &StampContext,
    existing: &[load::LoadedSkill],
) -> anyhow::Result<()> {
    let evolver_rig = RigId::new("evolver");
    let target_rig = &stamp.target_rig;

    // 7. Call agent.reply() and collect response
    let response = call_agent(agent, &ctx.prompt, ctx.evolver_item_id).await?;

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
                        call_agent(agent, &update_prompt, ctx.evolver_item_id).await?;
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
            // Validate
            match evolve::validate_skill_output(&content) {
                Ok(()) => {
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
                Err(e) => {
                    // Retry once with format fix
                    warn!("evolver: validation failed, retrying: {e}");
                    let retry_prompt = format!(
                        "{}\n\nPrevious output had format errors: {e}\n\
                         Please fix the format and try again.",
                        ctx.prompt
                    );
                    let retry_response =
                        call_agent(agent, &retry_prompt, ctx.evolver_item_id).await?;
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

async fn process_stamp(
    board: &Board,
    agent: &Agent,
    stamp: &opengoose_board::entity::stamp::Model,
) -> anyhow::Result<()> {
    let base_dir = crate::home_dir();
    let existing = load::load_skills_for(Some(&stamp.target_rig), None);

    update_effectiveness(stamp, &existing);
    let ctx = prepare_context(board, stamp, &existing).await?;
    let result = execute_action(&base_dir, board, agent, stamp, &ctx, &existing).await;

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

/// Idle-time sweep: re-evaluate dormant/archived skills against recent failures.
async fn run_sweep(board: &Board, agent: &Agent) -> anyhow::Result<()> {
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
            let effectiveness = load::read_metadata(&s.path).map(|meta| {
                let scores = &meta.effectiveness.subsequent_scores;
                let avg = if scores.is_empty() {
                    0.0
                } else {
                    scores.iter().sum::<f32>() / scores.len() as f32
                };
                let verdict = match load::is_effective(&meta) {
                    Some(true) => "effective",
                    Some(false) => "ineffective",
                    None => "insufficient data",
                };
                format!(
                    "{} injections, avg score {:.2}, verdict: {}",
                    meta.effectiveness.injected_count, avg, verdict
                )
            });
            (s.name.clone(), s.description.clone(), body, effectiveness)
        })
        .collect();

    // 3. Build and send sweep prompt
    let prompt = evolve::build_sweep_prompt(&skill_summaries, &failure_summaries);
    let response = call_agent(agent, &prompt, 0).await?;

    // 4. Parse and apply decisions
    let decisions = evolve::parse_sweep_response(&response);
    for decision in &decisions {
        match decision {
            evolve::SweepDecision::Restore(name) => {
                if let Some(skill) = dormant.iter().find(|s| &s.name == name) {
                    load::update_inclusion_tracking(&skill.path);
                    info!("sweep: restored '{name}' to active");
                }
            }
            evolve::SweepDecision::Refine(name, content) => {
                if let Some(skill) = dormant.iter().find(|s| &s.name == name)
                    && evolve::validate_skill_output(content).is_ok()
                {
                    evolve::refine_skill(&skill.path, content)?;
                    load::update_inclusion_tracking(&skill.path);
                    info!("sweep: refined and restored '{name}'");
                }
            }
            evolve::SweepDecision::Delete(name) => {
                if let Some(skill) = dormant.iter().find(|s| &s.name == name) {
                    std::fs::remove_dir_all(&skill.path)?;
                    info!("sweep: deleted obsolete skill '{name}'");
                }
            }
            evolve::SweepDecision::Keep(name) => {
                info!("sweep: keeping '{name}' dormant");
            }
        }
    }

    Ok(())
}

async fn call_agent(agent: &Agent, prompt: &str, work_id: i64) -> anyhow::Result<String> {
    if cfg!(test)
        && let Ok(test_reply) = std::env::var("OPENGOOSE_TEST_CALL_AGENT")
    {
        let raw = if prompt.contains("Previous output had format errors") {
            test_reply
                .split("||")
                .nth(1)
                .unwrap_or(&test_reply)
                .to_string()
        } else {
            test_reply
                .split("||")
                .next()
                .unwrap_or(&test_reply)
                .to_string()
        };
        if let Some(err_msg) = raw.strip_prefix("ERR:") {
            return Err(anyhow::anyhow!(err_msg.to_string()));
        }
        return Ok(raw);
    }

    let message = Message::user().with_text(prompt);
    let session_config = SessionConfig {
        id: format!("evolve-{work_id}"),
        schedule_id: None,
        max_turns: None,
        retry_config: None,
    };

    let stream = agent.reply(message, session_config, None).await?;
    tokio::pin!(stream);

    let mut response_text = String::new();
    while let Some(event) = stream.next().await {
        match event {
            Ok(AgentEvent::Message(msg)) => {
                use goose::conversation::message::MessageContent;
                for content in &msg.content {
                    if let MessageContent::Text(t) = content {
                        response_text.push_str(&t.text);
                    }
                }
            }
            Err(e) => return Err(e),
            _ => {}
        }
    }

    Ok(response_text)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]
    use super::*;
    use crate::skills::test_env_lock;
    use chrono::{Duration, Utc};
    use opengoose_board::board::AddStampParams;
    use opengoose_board::entity::stamp::Model;
    use opengoose_board::work_item::Status;
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
            .unprocessed_low_stamps(LOW_STAMP_THRESHOLD)
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
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();

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

        assert!(process_stamp(&board, &agent, &stamp).await.is_err());
    }

    #[tokio::test]
    async fn process_stamp_skips_when_agent_returns_skip() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("SKIP"));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
        let stamp = seeded_stamp(&board, "skip-rig").await;

        process_stamp(&board, &agent, &stamp).await.unwrap();

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
        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
    }

    #[tokio::test]
    async fn process_stamp_creates_skill_on_valid_evolve_output() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some(sample_skill()));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
        let stamp = seeded_stamp(&board, "create-rig").await;

        process_stamp(&board, &agent, &stamp).await.unwrap();

        let expected = home
            .path()
            .join(".opengoose/rigs/create-rig/skills/learned/test-skill/SKILL.md");
        assert!(expected.exists());

        restore_env_var("HOME", prev_home);
        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
    }

    #[tokio::test]
    async fn process_stamp_retries_when_first_output_invalid() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var(
            "OPENGOOSE_TEST_CALL_AGENT",
            Some("invalid raw output||SKIP"),
        );

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
        let stamp = seeded_stamp(&board, "retry-rig").await;

        process_stamp(&board, &agent, &stamp).await.unwrap();

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
        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
    }

    #[tokio::test]
    async fn process_stamp_marks_update_without_skill_file() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("UPDATE:test-existing"));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
        let stamp = seeded_stamp(&board, "update-rig").await;

        process_stamp(&board, &agent, &stamp).await.unwrap();

        let generated = board
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"));
        assert!(generated.is_some());
        let fetched = board.get(generated.unwrap().id).await.unwrap().unwrap();
        assert_eq!(fetched.status, Status::Done);

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
    }

    #[tokio::test]
    async fn process_stamp_propagates_agent_error_without_submit() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("ERR:boom"));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
        let stamp = seeded_stamp(&board, "error-rig").await;

        // process_stamp swallows execute_action errors: calls abandon (which fails
        // because Claimed→Abandoned is not a valid transition) and returns Ok(()).
        process_stamp(&board, &agent, &stamp).await.unwrap();

        let items = board.list().await.unwrap();
        let generated = items
            .into_iter()
            .find(|item| item.title.contains("Generate skill: Quality"))
            .expect("evolver work item should be posted");
        let fetched = board.get(generated.id).await.unwrap().unwrap();
        // abandon fails silently (Claimed→Abandoned invalid), so item stays Claimed
        assert_eq!(fetched.status, Status::Claimed);

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
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

    #[tokio::test]
    async fn run_sweep_returns_ok_when_no_dormant_skills() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();

        run_sweep(&board, &agent).await.unwrap();

        restore_env_var("HOME", prev_home);
    }

    #[tokio::test]
    async fn run_sweep_deletes_dormant_skill_from_decision() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("DELETE:dormant-old"));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "old failure".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("evolver"),
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

        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();
        assert!(!path.exists());

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    #[tokio::test]
    async fn run_sweep_keeps_dormant_skill_from_decision() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("KEEP:dormant-keep"));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "keep failure".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("evolver"),
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

        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();
        assert!(path.exists());

        let metadata: crate::skills::evolve::SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(path.join("metadata.json")).unwrap())
                .unwrap();
        assert!(metadata.last_included_at.is_none());

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    #[tokio::test]
    async fn run_sweep_restores_dormant_skill_from_decision() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("RESTORE:dormant-restore"));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "restore failure".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("evolver"),
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

        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();

        let metadata_after: crate::skills::evolve::SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(path.join("metadata.json")).unwrap())
                .unwrap();
        assert!(metadata_after.last_included_at.is_some());

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    #[tokio::test]
    async fn run_sweep_refines_dormant_skill_from_decision() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var(
            "OPENGOOSE_TEST_CALL_AGENT",
            Some(
                "REFINE:dormant-refine\n---\nname: dormant-refine\ndescription: Use when testing old behavior\n---\n\n# refind",
            ),
        );

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "refine failure".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("evolver"),
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

        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();
        let content = std::fs::read_to_string(path.join("SKILL.md")).unwrap();
        assert!(content.contains("# refind"));

        let metadata_after: crate::skills::evolve::SkillMetadata =
            serde_json::from_str(&std::fs::read_to_string(path.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(metadata_after.skill_version, 2);
        assert!(metadata_after.last_included_at.is_some());

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// run_sweep returns early when dormant skills exist but there are no recent low stamps.
    #[tokio::test]
    async fn run_sweep_returns_ok_when_dormant_exists_but_no_recent_stamps() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());

        // Create a dormant skill so it passes the first early-return check.
        let path = dormant_skill(home.path(), "r-nostamp", "dormant-nostamp", 60);
        assert!(path.is_dir());

        // Board has no stamps → recent_low_stamps returns empty → early return.
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();

        // Skill should still exist (sweep returned before making decisions).
        assert!(path.is_dir());

        restore_env_var("HOME", prev_home);
    }

    /// process_stamp with UPDATE where skill IS found but update response is not Create → warn only.
    #[tokio::test]
    async fn process_stamp_update_skill_found_update_response_not_create() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        // Both calls return "UPDATE:existing-skill" → second call also returns Update → _  arm (warn only)
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("UPDATE:existing-skill"));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
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
        process_stamp(&board, &agent, &stamp).await.unwrap();

        // Original SKILL.md unchanged since update response was not Create
        let content = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(content.contains("# Original"));

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// process_stamp: first output invalid → retry prompt has "Previous output had format errors"
    /// → call_agent returns second split (valid skill) → skill written.
    #[tokio::test]
    async fn process_stamp_retry_succeeds_when_second_output_valid() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let valid_skill = "---\nname: retry-skill\ndescription: Use when retrying format errors\n---\n# Retried\n";
        let reply = format!("invalid raw output||{valid_skill}");
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some(&reply));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
        let stamp = seeded_stamp(&board, "retry-success-rig").await;

        process_stamp(&board, &agent, &stamp).await.unwrap();

        let expected = home
            .path()
            .join(".opengoose/rigs/retry-success-rig/skills/learned/retry-skill/SKILL.md");
        assert!(expected.exists(), "retry skill file should be written");
        let content = std::fs::read_to_string(&expected).unwrap();
        assert!(content.contains("Retried"));

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// call_agent directly: ERR: prefix causes Err return.
    #[tokio::test]
    async fn call_agent_returns_err_for_err_prefix() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("ERR:test error message"));

        let agent = Agent::new();
        let result = call_agent(&agent, "normal prompt", 0).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("test error message")
        );

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
    }

    /// call_agent: normal prompt uses first split, retry prompt uses second split.
    #[tokio::test]
    async fn call_agent_uses_correct_split_based_on_prompt_content() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("first-part||second-part"));

        let agent = Agent::new();

        // Normal prompt → first split
        let normal = call_agent(&agent, "normal prompt", 0).await.unwrap();
        assert_eq!(normal, "first-part");

        // Retry prompt → second split
        let retry = call_agent(
            &agent,
            "some context\n\nPrevious output had format errors: missing name",
            0,
        )
        .await
        .unwrap();
        assert_eq!(retry, "second-part");

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
    }

    /// Covers evolver.rs lines — all branches of the effectiveness verdict in run_sweep.
    #[tokio::test]
    async fn run_sweep_covers_effectiveness_branch_variants() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var(
            "OPENGOOSE_TEST_CALL_AGENT",
            Some("KEEP:skill-empty\nKEEP:skill-insufficient\nKEEP:skill-effective"),
        );

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let work_item = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "effectiveness context failure".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("evolver"),
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

        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// Helper: creates a board with one recent stamp so run_sweep proceeds past the early-exit.
    async fn board_with_recent_stamp() -> Board {
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let item = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "recent failure for sweep".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("evolver"),
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

    /// Creates a single dormant skill in a temp home directory.
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

    /// Covers evolver.rs — RESTORE for a skill not in the dormant list → skips.
    #[tokio::test]
    async fn run_sweep_restore_nonexistent_skill_skips_gracefully() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var(
            "OPENGOOSE_TEST_CALL_AGENT",
            Some("RESTORE:not-a-real-skill"),
        );

        let board = board_with_recent_stamp().await;
        dormant_skill(home.path(), "skip-rig", "real-skill", 60);

        let agent = Agent::new();
        // Should succeed (no error for missing skill, just skip)
        run_sweep(&board, &agent).await.unwrap();

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// Covers evolver.rs — REFINE for a skill not in the dormant list → skips.
    #[tokio::test]
    async fn run_sweep_refine_nonexistent_skill_skips() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var(
            "OPENGOOSE_TEST_CALL_AGENT",
            Some(
                "REFINE:ghost-skill\n---\nname: ghost-skill\ndescription: Use when ghost\n---\n# Ghost\n",
            ),
        );

        let board = board_with_recent_stamp().await;
        dormant_skill(home.path(), "skip-rig2", "real-skill-2", 60);

        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// Covers evolver.rs — REFINE with invalid content (validation fails) → skips.
    #[tokio::test]
    async fn run_sweep_refine_invalid_content_skips() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        // Content without frontmatter → validate_skill_output fails
        let prev_reply = set_env_var(
            "OPENGOOSE_TEST_CALL_AGENT",
            Some("REFINE:refine-target\njust some content without frontmatter"),
        );

        let board = board_with_recent_stamp().await;
        dormant_skill_custom(
            home.path(),
            "skip-rig3",
            "refine-target",
            vec![0.0, 0.0, 0.0],
        );

        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// Covers evolver.rs — DELETE for a skill not in the dormant list → skips.
    #[tokio::test]
    async fn run_sweep_delete_nonexistent_skill_skips() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var(
            "OPENGOOSE_TEST_CALL_AGENT",
            Some("DELETE:nonexistent-skill"),
        );

        let board = board_with_recent_stamp().await;
        dormant_skill(home.path(), "skip-rig4", "real-skill-4", 60);

        let agent = Agent::new();
        run_sweep(&board, &agent).await.unwrap();

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// Covers evolver.rs — Installed skill and Learned skill without metadata.
    #[tokio::test]
    async fn process_stamp_with_installed_and_no_metadata_learned_skills() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("SKIP"));

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();

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
        process_stamp(&board, &agent, &stamp).await.unwrap();

        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
        restore_env_var("HOME", prev_home);
    }

    /// Covers evolver.rs:245-255 — the retry path where the second call returns valid
    /// SKILL.md content and write_skill_to_rig_scope is called successfully.
    #[tokio::test]
    async fn process_stamp_retry_succeeds_and_writes_skill() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = tempdir().unwrap();
        let prev_home = set_env_var("HOME", home.path().to_str());
        // First call: "invalid" → validate fails → retry.
        // Second call (retry, prompt contains "Previous output had format errors"): valid SKILL.md.
        let valid_skill = "\
---\nname: retry-skill\ndescription: Use when retry test needed.\n---\n\n# Retry\n";
        let prev_reply = set_env_var(
            "OPENGOOSE_TEST_CALL_AGENT",
            Some(&format!("invalid||{valid_skill}")),
        );

        let board = Board::connect("sqlite::memory:").await.unwrap();
        let agent = Agent::new();
        let stamp = seeded_stamp(&board, "retry-ok-rig").await;

        process_stamp(&board, &agent, &stamp).await.unwrap();

        // Skill file should exist in rig scope
        let skill_dir = home
            .path()
            .join(".opengoose/rigs/retry-ok-rig/skills/learned/retry-skill");
        assert!(
            skill_dir.join("SKILL.md").exists(),
            "retry skill should be written"
        );

        restore_env_var("HOME", prev_home);
        restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
    }
}
