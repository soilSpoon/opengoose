use std::collections::HashMap;

use anyhow::{Result, anyhow};
use tokio::task::JoinSet;
use tracing::{debug, warn};

use opengoose_profiles::ProfileStore;

use crate::chain_executor::{
    build_role_context, format_broadcast_context, get_or_create, load_history_pairs,
};
use crate::context::OrchestrationContext;
use crate::orchestrator::process_agent_communications;
use crate::runner::AgentRunner;
use crate::team::{MergeStrategy, TeamDefinition};

/// Executes the Fan-Out workflow: runs all agents in parallel, then
/// merges results according to the configured merge strategy.
pub struct FanOutExecutor<'a> {
    team: &'a TeamDefinition,
    profile_store: &'a ProfileStore,
    pool: &'a mut HashMap<String, AgentRunner>,
}

impl<'a> FanOutExecutor<'a> {
    pub fn new(
        team: &'a TeamDefinition,
        profile_store: &'a ProfileStore,
        pool: &'a mut HashMap<String, AgentRunner>,
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
        let fan_out_config = self
            .team
            .fan_out
            .as_ref()
            .ok_or_else(|| anyhow!("fan-out workflow requires fan_out config"))?;

        let session_key = ctx.session_key.to_stable_id();
        let team_run_id = ctx.team_run_id.clone();

        let history_pairs = load_history_pairs(ctx);

        let mut join_set = JoinSet::new();

        for (i, team_agent) in self.team.agents.iter().enumerate() {
            let profile = self
                .profile_store
                .get(&team_agent.profile)
                .map_err(|_| anyhow!("profile `{}` not found", team_agent.profile))?;

            let step_id = ctx.work_items().create(
                &session_key,
                &team_run_id,
                &format!("Fan-out: {}", team_agent.profile),
                Some(parent_id),
            )?;
            ctx.work_items()
                .assign(step_id, &team_agent.profile, Some(i as i32))?;

            let role_ctx = build_role_context(team_agent.role.as_deref(), "Your role");

            let agent_input = format!(
                "{input}{role_ctx}\n\n\
                 [You are part of a parallel team. If you make important discoveries, \
                 prefix them with [BROADCAST]: so other agents can see them.]"
            );
            let profile_name = team_agent.profile.clone();
            let history = history_pairs.clone();

            // Fan-out tasks need owned runners (moved into spawned futures).
            join_set.spawn(async move {
                let runner = AgentRunner::from_profile(&profile).await?;
                if !history.is_empty()
                    && let Err(e) = runner.seed_history(&history).await
                {
                    warn!("failed to seed history for fan-out agent: {e}");
                }
                let output = runner.run(&agent_input).await?;
                Ok::<(String, i32, crate::runner::AgentOutput), anyhow::Error>((
                    profile_name,
                    step_id,
                    output,
                ))
            });
        }

        // Collect results
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let (profile_name, step_id, output) = result??;
            debug!(profile = %profile_name, "fan-out agent complete");

            process_agent_communications(self.team, ctx, &session_key, &profile_name, &output);

            ctx.work_items().set_output(step_id, &output.response)?;

            results.push((profile_name, output.response));
        }

        // Merge
        match fan_out_config.merge_strategy {
            MergeStrategy::Concatenate => {
                let merged = results
                    .iter()
                    .map(|(name, output)| format!("## {name}\n\n{output}"))
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");
                Ok(merged)
            }
            MergeStrategy::Summary => {
                let combined = results
                    .iter()
                    .map(|(name, output)| format!("### {name}\n{output}"))
                    .collect::<Vec<_>>()
                    .join("\n\n");

                let broadcast_section = format_broadcast_context(ctx, "**Team broadcasts:**");

                let summary_input = format!(
                    "Multiple agents investigated the following question:\n\n\
                     **Original question:** {input}\n\n\
                     **Agent results:**\n\n{combined}{broadcast_section}\n\n\
                     Please synthesize these results into a single coherent response."
                );

                let first_profile = self
                    .profile_store
                    .get(&self.team.agents[0].profile)
                    .map_err(|_| anyhow!("profile `{}` not found", self.team.agents[0].profile))?;

                let runner = get_or_create(self.pool, &first_profile).await?;
                let output = runner.run(&summary_input).await?;
                Ok(output.response)
            }
        }
    }
}
