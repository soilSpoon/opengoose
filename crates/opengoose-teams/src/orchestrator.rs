use std::collections::HashMap;

use anyhow::{Result, anyhow};
use tokio::sync::Mutex;
use tracing::{info, warn};

use opengoose_persistence::{MessageType, WorkStatus};
use opengoose_profiles::ProfileStore;
use opengoose_types::AppEventKind;

use crate::chain_executor::{self, ChainExecutor};
use crate::context::OrchestrationContext;
use crate::fan_out_executor::FanOutExecutor;
use crate::router_executor::RouterExecutor;
use crate::runner::{AgentOutput, AgentRunner};
use crate::team::{OrchestrationPattern, TeamDefinition};

/// Maximum delegation recursion depth to prevent infinite loops.
const MAX_DELEGATION_DEPTH: usize = 3;

#[derive(Debug, Default)]
struct DelegationOutcome {
    succeeded: usize,
    failed: usize,
}

/// Executes a team workflow by orchestrating multiple agent runners.
///
/// The internal agent pool is persistent: runners created for one message are
/// reused for subsequent messages in the same session, avoiding MCP extension
/// restarts between turns.
pub struct TeamOrchestrator {
    team: TeamDefinition,
    profile_store: ProfileStore,
    /// Per-session agent pool, keyed by agent profile name.
    /// Shared across `execute` and `resume` calls so extensions stay loaded.
    pool: Mutex<HashMap<String, AgentRunner>>,
}

impl TeamOrchestrator {
    pub fn new(team: TeamDefinition, profile_store: ProfileStore) -> Self {
        Self {
            team,
            profile_store,
            pool: Mutex::new(HashMap::new()),
        }
    }

    pub async fn execute(&self, input: &str, ctx: &OrchestrationContext) -> Result<String> {
        info!(team = %self.team.name(), workflow = ?self.team.workflow, "executing team");

        let session_key = ctx.session_key.to_stable_id();
        let workflow_str: &str = match self.team.workflow {
            OrchestrationPattern::Chain => "chain",
            OrchestrationPattern::FanOut => "fan_out",
            OrchestrationPattern::Router => "router",
        };

        ctx.orchestration().create_run(
            &ctx.team_run_id,
            &session_key,
            self.team.name(),
            workflow_str,
            input,
            self.team.agents.len() as i32,
        )?;

        ctx.emit(AppEventKind::TeamRunStarted {
            team: self.team.name().to_string(),
            workflow: workflow_str.to_string(),
            input: input.to_string(),
        });

        let parent_id = ctx.work_items().create(
            &session_key,
            &ctx.team_run_id,
            &format!("Team: {}", self.team.name()),
            None,
        )?;
        ctx.work_items().set_input(parent_id, input)?;
        ctx.work_items()
            .update_status(parent_id, WorkStatus::InProgress)?;

        let mut pool = self.pool.lock().await;

        let result = match self.team.workflow {
            OrchestrationPattern::Chain => {
                ChainExecutor::new(&self.team, &self.profile_store, &mut pool)
                    .execute(input, ctx, parent_id)
                    .await
            }
            OrchestrationPattern::FanOut => {
                FanOutExecutor::new(&self.team, &self.profile_store, &mut pool)
                    .execute(input, ctx, parent_id)
                    .await
            }
            OrchestrationPattern::Router => {
                RouterExecutor::new(&self.team, &self.profile_store, &mut pool)
                    .execute(input, ctx, parent_id)
                    .await
            }
        };

        if result.is_ok() {
            match self
                .process_pending_delegations(ctx, parent_id, 0, &mut pool)
                .await
            {
                Ok(outcome) => {
                    if outcome.failed > 0 {
                        info!(count = outcome.failed, "some delegations failed");
                    }
                }
                Err(e) => warn!(%e, "delegation processing error"),
            }
        }

        let dead = match ctx.queue().get_dead_letters(&ctx.team_run_id) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "failed to retrieve dead letters for run {}: {e}",
                    ctx.team_run_id
                );
                Default::default()
            }
        };

        match &result {
            Ok(response) => {
                ctx.work_items().set_output(parent_id, response)?;
                ctx.orchestration()
                    .complete_run(&ctx.team_run_id, response)?;
                ctx.emit(AppEventKind::TeamRunCompleted {
                    team: self.team.name().to_string(),
                });
            }
            Err(e) => {
                let err_msg = e.to_string();
                ctx.work_items().set_error(parent_id, &err_msg)?;
                ctx.orchestration().fail_run(&ctx.team_run_id, &err_msg)?;
                ctx.emit(AppEventKind::TeamRunFailed {
                    team: self.team.name().to_string(),
                    reason: err_msg,
                });
            }
        }

        // Persist extension state for all pooled runners so connections can be
        // restored if the process restarts or the pool is evicted.
        for runner in pool.values() {
            if let Err(e) = runner.save_extension_state().await {
                warn!(
                    profile = %runner.profile_name(),
                    error = %e,
                    "failed to save extension state (non-fatal)"
                );
            }
        }

        let mut final_response = result?;
        if !dead.is_empty() {
            let notes = dead
                .iter()
                .map(|d| {
                    format!(
                        "- {} \u{2192} {}: {}",
                        d.sender,
                        d.recipient,
                        d.error.as_deref().unwrap_or("unknown error")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            final_response.push_str(&format!("\n\n---\n**Failed delegations:**\n{notes}"));
        }

        Ok(final_response)
    }

    pub async fn resume(&self, ctx: &OrchestrationContext, parent_work_id: i32) -> Result<String> {
        if self.team.workflow != OrchestrationPattern::Chain {
            return Err(anyhow!(
                "only chain workflows support resume (this team uses {:?})",
                self.team.workflow
            ));
        }

        info!(team = %self.team.name(), parent_work_id, "resuming team execution");

        let resume_point = ctx.work_items().find_resume_point(parent_work_id)?;
        let (start_step, last_output) = match resume_point {
            Some(point) => point,
            None => {
                let parent = ctx
                    .work_items()
                    .get(parent_work_id)?
                    .ok_or_else(|| anyhow!("parent work item {} not found", parent_work_id))?;
                let original_input = parent.input.unwrap_or_default();
                (0, original_input)
            }
        };

        ctx.orchestration().resume_run(&ctx.team_run_id)?;
        ctx.orchestration()
            .advance_step(&ctx.team_run_id, start_step)?;

        let mut pool = self.pool.lock().await;

        let result = ChainExecutor::new(&self.team, &self.profile_store, &mut pool)
            .execute_from_step(&last_output, ctx, parent_work_id, start_step as usize)
            .await;

        match &result {
            Ok(response) => {
                ctx.work_items().set_output(parent_work_id, response)?;
                ctx.orchestration()
                    .complete_run(&ctx.team_run_id, response)?;
            }
            Err(e) => {
                let err_msg = e.to_string();
                ctx.work_items().set_error(parent_work_id, &err_msg)?;
                ctx.orchestration().fail_run(&ctx.team_run_id, &err_msg)?;
            }
        }

        result
    }

    async fn process_pending_delegations(
        &self,
        ctx: &OrchestrationContext,
        parent_work_id: i32,
        depth: usize,
        pool: &mut HashMap<String, AgentRunner>,
    ) -> Result<DelegationOutcome> {
        if depth >= MAX_DELEGATION_DEPTH {
            info!(depth, "max delegation depth reached, stopping");
            return Ok(DelegationOutcome::default());
        }

        let session_key = ctx.session_key.to_stable_id();
        let mut outcome = DelegationOutcome::default();

        let delegations = ctx
            .queue()
            .dequeue_delegations(&ctx.team_run_id, 50)
            .map_err(|e| anyhow!("failed to dequeue delegations: {e}"))?;

        if delegations.is_empty() {
            return Ok(outcome);
        }

        for msg in delegations {
            let work_id = ctx.work_items().create(
                &session_key,
                &ctx.team_run_id,
                &format!("Delegation: {} \u{2192} {}", msg.sender, msg.recipient),
                Some(parent_work_id),
            )?;
            ctx.work_items().assign(work_id, &msg.recipient, None)?;
            ctx.work_items().set_input(work_id, &msg.content)?;

            let profile = match self.profile_store.get(&msg.recipient) {
                Ok(p) => p,
                Err(_) => {
                    let err = format!("profile '{}' not found", msg.recipient);
                    ctx.work_items().set_error(work_id, &err)?;
                    if let Err(e) = ctx.queue().fail(msg.id, &err) {
                        warn!("failed to mark delegation as failed: {e}");
                    }
                    outcome.failed += 1;
                    continue;
                }
            };

            let delegation_input = format!("[Delegated from {}]: {}", msg.sender, msg.content);

            info!(
                sender = %msg.sender,
                recipient = %msg.recipient,
                depth,
                "executing delegation"
            );

            let project = ctx.project_context.as_deref();
            match chain_executor::get_or_create(pool, &profile, &session_key, project).await {
                Ok(runner) => match runner.run(&delegation_input).await {
                    Ok(output) => {
                        process_agent_communications(
                            &self.team,
                            ctx,
                            &session_key,
                            &msg.recipient,
                            &output,
                        );
                        ctx.work_items().set_output(work_id, &output.response)?;
                        if let Err(e) = ctx.queue().complete(msg.id) {
                            warn!("failed to mark delegation message as complete: {e}");
                        }
                        outcome.succeeded += 1;
                    }
                    Err(e) => {
                        ctx.work_items().set_error(work_id, &e.to_string())?;
                        if let Err(qe) = ctx.queue().fail(msg.id, &e.to_string()) {
                            warn!("failed to mark delegation as failed: {qe}");
                        }
                        outcome.failed += 1;
                    }
                },
                Err(e) => {
                    ctx.work_items().set_error(work_id, &e.to_string())?;
                    if let Err(qe) = ctx.queue().fail(msg.id, &e.to_string()) {
                        warn!("failed to mark delegation as failed: {qe}");
                    }
                    outcome.failed += 1;
                }
            }
        }

        let sub = Box::pin(self.process_pending_delegations(ctx, parent_work_id, depth + 1, pool))
            .await?;
        outcome.succeeded += sub.succeeded;
        outcome.failed += sub.failed;

        Ok(outcome)
    }
}

// ── Shared helpers (pub(crate) for use by executors) ─────────────────

pub(crate) fn is_team_member(team: &TeamDefinition, agent_name: &str) -> bool {
    team.agents.iter().any(|a| a.profile == agent_name)
}

pub(crate) fn process_agent_communications(
    team: &TeamDefinition,
    ctx: &OrchestrationContext,
    session_key: &str,
    agent_name: &str,
    output: &AgentOutput,
) {
    for broadcast in &output.broadcasts {
        ctx.broadcast(agent_name, broadcast);
    }
    enqueue_validated_delegations(team, ctx, session_key, agent_name, &output.delegations);
}

fn enqueue_validated_delegations(
    team: &TeamDefinition,
    ctx: &OrchestrationContext,
    session_key: &str,
    sender: &str,
    delegations: &[(String, String)],
) {
    for (recipient, msg) in delegations {
        if recipient == sender {
            info!(
                %sender,
                "self-delegation rejected (would cause cycle)"
            );
            continue;
        }
        if is_team_member(team, recipient) {
            if let Err(e) = ctx.queue().enqueue(
                session_key,
                &ctx.team_run_id,
                sender,
                recipient,
                msg,
                MessageType::Delegation,
            ) {
                warn!("failed to enqueue delegation from {sender} to {recipient}: {e}");
            }
        } else {
            info!(
                %sender,
                %recipient,
                "delegation to unknown agent rejected"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use opengoose_persistence::Database;
    use opengoose_types::{EventBus, Platform, SessionKey};

    use crate::team::{OrchestrationPattern, TeamAgent};

    fn test_team() -> TeamDefinition {
        TeamDefinition {
            version: "1.0.0".into(),
            title: "test-team".into(),
            description: None,
            goal: None,
            workflow: OrchestrationPattern::Chain,
            agents: vec![
                TeamAgent {
                    profile: "coder".into(),
                    role: Some("write code".into()),
                },
                TeamAgent {
                    profile: "reviewer".into(),
                    role: Some("review code".into()),
                },
            ],
            router: None,
            fan_out: None,
        }
    }

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
    fn test_is_team_member_found() {
        let team = test_team();
        assert!(is_team_member(&team, "coder"));
        assert!(is_team_member(&team, "reviewer"));
    }

    #[test]
    fn test_is_team_member_not_found() {
        let team = test_team();
        assert!(!is_team_member(&team, "unknown"));
    }

    #[test]
    fn test_process_agent_communications_broadcasts() {
        let team = test_team();
        let ctx = test_ctx();
        let output = AgentOutput {
            response: "done".into(),
            delegations: vec![],
            broadcasts: vec!["found bug".into()],
        };
        process_agent_communications(&team, &ctx, "sess1", "coder", &output);
        let broadcasts = ctx.read_broadcasts(None);
        assert_eq!(broadcasts.len(), 1);
        assert_eq!(broadcasts[0].content, "found bug");
    }

    #[test]
    fn test_self_delegation_rejected() {
        let team = test_team();
        let ctx = test_ctx();
        let output = AgentOutput {
            response: "done".into(),
            delegations: vec![("coder".into(), "delegate to self".into())],
            broadcasts: vec![],
        };
        process_agent_communications(&team, &ctx, "sess1", "coder", &output);
        // Self-delegation should be rejected, so no messages in queue
        let msgs = ctx
            .queue()
            .dequeue_delegations(&ctx.team_run_id, 10)
            .unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_unknown_agent_delegation_rejected() {
        let team = test_team();
        let ctx = test_ctx();
        let output = AgentOutput {
            response: "done".into(),
            delegations: vec![("unknown_agent".into(), "do something".into())],
            broadcasts: vec![],
        };
        process_agent_communications(&team, &ctx, "sess1", "coder", &output);
        let msgs = ctx
            .queue()
            .dequeue_delegations(&ctx.team_run_id, 10)
            .unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_valid_delegation_enqueued() {
        let team = test_team();
        let ctx = test_ctx();
        let session_key = ctx.session_key.to_stable_id();
        let output = AgentOutput {
            response: "done".into(),
            delegations: vec![("reviewer".into(), "please review".into())],
            broadcasts: vec![],
        };
        process_agent_communications(&team, &ctx, &session_key, "coder", &output);
        let msgs = ctx
            .queue()
            .dequeue_delegations(&ctx.team_run_id, 10)
            .unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "please review");
        assert_eq!(msgs[0].recipient, "reviewer");
    }
}
