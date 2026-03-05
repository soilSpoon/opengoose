use anyhow::Result;
use tracing::debug;

use opengoose_profiles::ProfileStore;
use opengoose_types::AppEventKind;

use crate::agent_pool::AgentPool;
use crate::context::OrchestrationContext;
use crate::orchestrator::process_agent_communications;
use crate::prompt_context::PromptContextBuilder;
use crate::team::TeamDefinition;

/// Executes the Chain workflow: runs agents sequentially, piping output
/// from one to the next.
pub struct ChainExecutor<'a> {
    team: &'a TeamDefinition,
    profile_store: &'a ProfileStore,
    pool: &'a mut AgentPool,
}

impl<'a> ChainExecutor<'a> {
    pub fn new(
        team: &'a TeamDefinition,
        profile_store: &'a ProfileStore,
        pool: &'a mut AgentPool,
    ) -> Self {
        Self {
            team,
            profile_store,
            pool,
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

        let prompt_ctx = PromptContextBuilder::new(
            ctx,
            None,
            "Your role in this team",
            "Team findings so far",
        );
        let history_text = prompt_ctx.history_text().to_string();

        for (i, team_agent) in self.team.agents.iter().enumerate().skip(start_step) {
            let profile = self
                .profile_store
                .get(&team_agent.profile)
                .map_err(|_| anyhow::anyhow!("profile `{}` not found", team_agent.profile))?;

            let agent_prompt_ctx = PromptContextBuilder::new(
                ctx,
                team_agent.role.as_deref(),
                "Your role in this team",
                "Team findings so far",
            );

            // Create work item for this step
            let step_id = ctx.work_items().create(
                &session_key,
                &ctx.team_run_id,
                &format!("Step {i}: {}", team_agent.profile),
                Some(parent_id),
            )?;
            ctx.work_items()
                .assign(step_id, &team_agent.profile, Some(i as i32))?;
            ctx.work_items().set_input(step_id, &current)?;

            let runner = self.pool.get_or_create(&profile).await?;

            let step_input = if i == start_step && start_step == 0 {
                let history_prefix =
                    crate::prompt_context::build_history_prefix(&history_text);
                format!(
                    "{history_prefix}{current}{}{}",
                    agent_prompt_ctx.role_ctx(),
                    agent_prompt_ctx.broadcast_ctx()
                )
            } else {
                format!(
                    "Previous agent's output:\n---\n{current}\n---\n\
                     Please continue based on the above.{}{}",
                    agent_prompt_ctx.role_ctx(),
                    agent_prompt_ctx.broadcast_ctx()
                )
            };

            debug!(step = i, profile = %team_agent.profile, "chain step");

            ctx.orchestration()
                .advance_step(&ctx.team_run_id, i as i32)?;

            ctx.emit(AppEventKind::TeamStepStarted {
                team: self.team.name().to_string(),
                agent: team_agent.profile.clone(),
                step: i,
            });

            match runner.run(&step_input).await {
                Ok(output) => {
                    process_agent_communications(
                        self.team,
                        ctx,
                        &session_key,
                        &team_agent.profile,
                        &output,
                    );
                    ctx.work_items().set_output(step_id, &output.response)?;
                    ctx.emit(AppEventKind::TeamStepCompleted {
                        team: self.team.name().to_string(),
                        agent: team_agent.profile.clone(),
                    });
                    current = output.response;
                }
                Err(e) => {
                    ctx.work_items()
                        .set_error(step_id, &e.to_string())?;
                    ctx.emit(AppEventKind::TeamStepFailed {
                        team: self.team.name().to_string(),
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
