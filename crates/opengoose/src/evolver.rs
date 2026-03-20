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

const EVOLVER_SYSTEM_PROMPT: &str =
    "You are a skill analyst for OpenGoose.\n\
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
                            return Err(anyhow::anyhow!(
                                "retry failed, item marked stuck"
                            ));
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
