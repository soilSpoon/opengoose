use anyhow::{Result, anyhow};
use tracing::{info, instrument, warn};

use opengoose_persistence::WorkStatus;
use opengoose_types::AppEventKind;

use crate::chain_executor::ChainExecutor;
use crate::context::OrchestrationContext;
use crate::fan_out_executor::FanOutExecutor;
use crate::router_executor::RouterExecutor;
use crate::team::OrchestrationPattern;

use super::TeamOrchestrator;

impl TeamOrchestrator {
    #[instrument(
        skip(self, input, ctx),
        fields(
            team = %self.team.name(),
            workflow = ?self.team.workflow,
            session_id = %ctx.session_key.to_stable_id(),
            input_len = input.chars().count()
        )
    )]
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
                ChainExecutor::new(
                    &self.team,
                    &self.profile_store,
                    &mut pool,
                    self.model_override.as_deref(),
                )
                .execute(input, ctx, parent_id)
                .await
            }
            OrchestrationPattern::FanOut => {
                FanOutExecutor::new(
                    &self.team,
                    &self.profile_store,
                    &mut pool,
                    self.model_override.as_deref(),
                )
                .execute(input, ctx, parent_id)
                .await
            }
            OrchestrationPattern::Router => {
                RouterExecutor::new(
                    &self.team,
                    &self.profile_store,
                    &mut pool,
                    self.model_override.as_deref(),
                )
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

    #[instrument(
        skip(self, ctx),
        fields(
            team = %self.team.name(),
            workflow = ?self.team.workflow,
            session_id = %ctx.session_key.to_stable_id(),
            parent_work_id
        )
    )]
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

        let result = ChainExecutor::new(
            &self.team,
            &self.profile_store,
            &mut pool,
            self.model_override.as_deref(),
        )
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
}
