use std::collections::HashMap;

use anyhow::{Context, Result};
use tracing::{debug, warn};

use opengoose_profiles::ProfileStore;
use opengoose_types::AppEventKind;

use crate::context::OrchestrationContext;
use crate::executor_context::{ExecutorContext, inject_team_role, resolve_profile};
use crate::orchestrator::process_agent_communications;
use crate::runner::AgentRunner;
use crate::team::TeamDefinition;

/// Executes the Chain workflow: runs agents sequentially, piping output
/// from one to the next.
pub struct ChainExecutor<'a> {
    ctx: ExecutorContext<'a>,
}

impl<'a> ChainExecutor<'a> {
    pub fn new(
        team: &'a TeamDefinition,
        profile_store: &'a ProfileStore,
        pool: &'a mut HashMap<String, AgentRunner>,
    ) -> Self {
        Self {
            ctx: ExecutorContext::new(team, profile_store, pool),
        }
    }

    pub async fn execute(
        &mut self,
        input: &str,
        ctx: &OrchestrationContext,
        parent_id: i32,
    ) -> Result<String> {
        self.execute_from_step(input, ctx, parent_id, 0).await
    }

    pub async fn execute_from_step(
        &mut self,
        input: &str,
        ctx: &OrchestrationContext,
        parent_id: i32,
        start_step: usize,
    ) -> Result<String> {
        let mut current = input.to_string();
        let session_key = ctx.session_key.to_stable_id();

        let history_pairs = load_history_pairs(ctx);

        for (i, team_agent) in self.ctx.team.agents.iter().enumerate().skip(start_step) {
            let profile = resolve_profile(self.ctx.profile_store, &team_agent.profile)?;

            let step_id = ctx.work_items().create(
                &session_key,
                &ctx.team_run_id,
                &format!("Step {i}: {}", team_agent.profile),
                Some(parent_id),
            )?;
            ctx.work_items()
                .assign(step_id, &team_agent.profile, Some(i as i32))?;
            ctx.work_items().set_input(step_id, &current)?;

            let project = ctx.project_context.as_deref();
            let runner =
                get_or_create(self.ctx.pool, &profile, &ctx.session_key.to_stable_id(), project).await?;

            // Inject team context into system prompt (keyed, additive)
            if let Some(role) = &team_agent.role {
                inject_team_role(runner, role).await;
            }
            let broadcast_ctx = format_broadcast_context(ctx, "Team findings so far");
            if !broadcast_ctx.is_empty() {
                runner
                    .extend_system_prompt("team_broadcasts", &broadcast_ctx)
                    .await;
            }

            if i == start_step
                && start_step == 0
                && !history_pairs.is_empty()
                && let Err(e) = runner.seed_history(&history_pairs).await
            {
                warn!("failed to seed history into Goose session: {e}");
            }

            let step_input = if i == start_step && start_step == 0 {
                current.clone()
            } else {
                format!(
                    "Previous agent's output:\n---\n{current}\n---\n\
                     Please continue based on the above."
                )
            };

            debug!(step = i, profile = %team_agent.profile, "chain step");

            ctx.orchestration()
                .advance_step(&ctx.team_run_id, i as i32)?;

            ctx.emit(AppEventKind::TeamStepStarted {
                team: self.ctx.team.name().to_string(),
                agent: team_agent.profile.clone(),
                step: i,
            });

            match runner.run(&step_input).await {
                Ok(output) => {
                    process_agent_communications(
                        self.ctx.team,
                        ctx,
                        &session_key,
                        &team_agent.profile,
                        &output,
                    );
                    ctx.work_items().set_output(step_id, &output.response)?;
                    ctx.emit(AppEventKind::TeamStepCompleted {
                        team: self.ctx.team.name().to_string(),
                        agent: team_agent.profile.clone(),
                    });
                    current = output.response;
                }
                Err(e) => {
                    ctx.work_items().set_error(step_id, &e.to_string())?;
                    ctx.emit(AppEventKind::TeamStepFailed {
                        team: self.ctx.team.name().to_string(),
                        agent: team_agent.profile.clone(),
                        reason: e.to_string(),
                    });
                    return Err(e);
                }
            }
        }

        Ok(current)
    }
}

// ── Shared helpers ──────────────────────────────────────────────────

/// Get or create an `AgentRunner` from the pool.
///
/// If a runner for `profile.name()` already exists in `pool` it is reused
/// (MCP extensions stay connected between calls).  Otherwise a new runner is
/// created with a deterministic session ID derived from `session_prefix` and
/// the profile name so that Goose can restore message history on reconnect.
///
/// When `project` is `Some`, the new runner inherits the project's `cwd` and
/// has the project goal / context files injected into its system prompt.
pub(crate) async fn get_or_create<'a>(
    pool: &'a mut HashMap<String, AgentRunner>,
    profile: &opengoose_profiles::AgentProfile,
    session_prefix: &str,
    project: Option<&opengoose_projects::ProjectContext>,
) -> Result<&'a AgentRunner> {
    let name = profile.name().to_string();
    if !pool.contains_key(&name) {
        let session_id = format!("{session_prefix}::{name}");
        let runner = AgentRunner::from_profile_keyed_with_project(
            profile,
            session_id,
            project,
        )
        .await?;
        pool.insert(name.clone(), runner);
    }
    pool.get(&name)
        .context("agent runner pool lost entry immediately after insertion")
}

pub(crate) fn load_history_pairs(ctx: &OrchestrationContext) -> Vec<(String, String)> {
    match ctx.sessions().load_history(&ctx.session_key, 20) {
        Ok(history) => history.into_iter().map(|h| (h.role, h.content)).collect(),
        Err(e) => {
            warn!("failed to load conversation history: {e}");
            Vec::new()
        }
    }
}

pub(crate) fn format_broadcast_context(ctx: &OrchestrationContext, header: &str) -> String {
    let broadcasts = ctx.read_broadcasts(None);
    if broadcasts.is_empty() {
        String::new()
    } else {
        let text: String = broadcasts
            .iter()
            .map(|b| format!("- [{}]: {}", b.sender, b.content))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\n[{header}]:\n{text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use opengoose_persistence::Database;
    use opengoose_types::{EventBus, Platform, SessionKey};

    fn test_ctx() -> OrchestrationContext {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let bus = EventBus::new(16);
        let key = SessionKey::new(Platform::Discord, "g1", "ch1");
        let ctx = OrchestrationContext::new("run-1".into(), key, db, bus);
        // Ensure session exists for FK constraints on message_queue
        ctx.sessions()
            .append_user_message(&ctx.session_key, "init", None)
            .unwrap();
        ctx
    }

    #[test]
    fn test_format_broadcast_context_empty() {
        let ctx = test_ctx();
        let result = format_broadcast_context(&ctx, "Header");
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_broadcast_context_with_items() {
        let ctx = test_ctx();
        // Enqueue a broadcast message
        ctx.broadcast("coder", "found a bug");
        let result = format_broadcast_context(&ctx, "Team findings");
        assert!(result.contains("[Team findings]:"));
        assert!(result.contains("- [coder]: found a bug"));
    }

    #[test]
    fn test_load_history_pairs_empty() {
        // Use a fresh context without seeded session data
        let db = Arc::new(Database::open_in_memory().unwrap());
        let bus = EventBus::new(16);
        let key = SessionKey::new(Platform::Discord, "g2", "ch2");
        let ctx = OrchestrationContext::new("run-2".into(), key, db, bus);
        let pairs = load_history_pairs(&ctx);
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_load_history_pairs_with_data() {
        let ctx = test_ctx();
        // test_ctx already inserted an "init" message, add two more
        ctx.sessions()
            .append_user_message(&ctx.session_key, "hi", Some("alice"))
            .unwrap();
        ctx.sessions()
            .append_assistant_message(&ctx.session_key, "hello")
            .unwrap();
        let pairs = load_history_pairs(&ctx);
        assert_eq!(pairs.len(), 3); // init + hi + hello
        assert_eq!(pairs[1], ("user".to_string(), "hi".to_string()));
        assert_eq!(pairs[2], ("assistant".to_string(), "hello".to_string()));
    }
}
