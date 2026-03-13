use std::collections::HashMap;

use anyhow::{Result, anyhow};
use tracing::{info, instrument, warn};

use crate::chain_executor;
use crate::context::OrchestrationContext;
use crate::executor_context::resolve_profile;
use crate::runner::AgentRunner;

use super::helpers::process_agent_communications;
use super::{MAX_DELEGATION_DEPTH, TeamOrchestrator};

#[derive(Debug, Default)]
pub(crate) struct DelegationOutcome {
    pub succeeded: usize,
    pub failed: usize,
}

impl TeamOrchestrator {
    #[instrument(
        skip(self, ctx, pool),
        fields(
            team = %self.team.name(),
            session_id = %ctx.session_key.to_stable_id(),
            parent_work_id,
            depth
        )
    )]
    pub(crate) async fn process_pending_delegations(
        &self,
        ctx: &OrchestrationContext,
        parent_work_id: &str,
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
            );
            ctx.work_items().assign(&work_id, &msg.recipient, None);
            ctx.work_items().set_input(&work_id, &msg.content);

            let profile = match resolve_profile(
                &self.profile_store,
                &msg.recipient,
                self.model_override.as_deref(),
            ) {
                Ok(p) => p,
                Err(_) => {
                    let err = format!("profile '{}' not found", msg.recipient);
                    ctx.work_items().set_error(&work_id, &err);
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
                        ctx.work_items().set_output(&work_id, &output.response);
                        if let Err(e) = ctx.queue().complete(msg.id) {
                            warn!("failed to mark delegation message as complete: {e}");
                        }
                        outcome.succeeded += 1;
                    }
                    Err(e) => {
                        ctx.work_items().set_error(&work_id, &e.to_string());
                        if let Err(qe) = ctx.queue().fail(msg.id, &e.to_string()) {
                            warn!("failed to mark delegation as failed: {qe}");
                        }
                        outcome.failed += 1;
                    }
                },
                Err(e) => {
                    ctx.work_items().set_error(&work_id, &e.to_string());
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
