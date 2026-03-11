use std::collections::HashMap;

use anyhow::{Result, anyhow};
use tokio::task::JoinSet;
use tracing::{debug, warn};

use opengoose_profiles::ProfileStore;

use crate::chain_executor::{format_broadcast_context, get_or_create, load_history_pairs};
use crate::context::OrchestrationContext;
use crate::executor_context::{ExecutorContext, inject_team_role, resolve_profile};
use crate::orchestrator::process_agent_communications;
use crate::runner::AgentRunner;
use crate::team::{MergeStrategy, TeamDefinition};

/// Executes the Fan-Out workflow: runs all agents in parallel, then
/// merges results according to the configured merge strategy.
pub struct FanOutExecutor<'a> {
    ctx: ExecutorContext<'a>,
}

impl<'a> FanOutExecutor<'a> {
    pub fn new(
        team: &'a TeamDefinition,
        profile_store: &'a ProfileStore,
        pool: &'a mut HashMap<String, AgentRunner>,
        model_override: Option<&'a str>,
    ) -> Self {
        Self {
            ctx: ExecutorContext::new(team, profile_store, pool, model_override),
        }
    }

    pub async fn execute(
        &mut self,
        input: &str,
        ctx: &OrchestrationContext,
        parent_id: i32,
    ) -> Result<String> {
        let fan_out_config = self
            .ctx
            .team
            .fan_out
            .as_ref()
            .ok_or_else(|| anyhow!("fan-out workflow requires fan_out config"))?;

        let session_key = ctx.session_key.to_stable_id();
        let team_run_id = ctx.team_run_id.clone();

        let history_pairs = load_history_pairs(ctx);

        let mut join_set = JoinSet::new();

        for (i, team_agent) in self.ctx.team.agents.iter().enumerate() {
            let profile = resolve_profile(
                self.ctx.profile_store,
                &team_agent.profile,
                self.ctx.model_override,
            )?;

            let step_id = ctx.work_items().create(
                &session_key,
                &team_run_id,
                &format!("Fan-out: {}", team_agent.profile),
                Some(parent_id),
            )?;
            ctx.work_items()
                .assign(step_id, &team_agent.profile, Some(i as i32))?;

            let agent_input = format!(
                "{input}\n\n\
                 [You are part of a parallel team. If you make important discoveries, \
                 prefix them with [BROADCAST]: so other agents can see them.]"
            );
            let profile_name = team_agent.profile.clone();
            let role = team_agent.role.clone();
            let history = history_pairs.clone();
            // Deterministic session_id: same agent for same session reuses its
            // Goose session (message history preserved between invocations).
            let session_id = format!("{session_key}::{}", team_agent.profile);
            // Clone project context for the spawned task (cheap Arc clone).
            let project_ctx = ctx.project_context.clone();

            // Fan-out tasks need owned runners (moved into spawned futures).
            join_set.spawn(async move {
                let runner = AgentRunner::from_profile_keyed_with_project(
                    &profile,
                    session_id,
                    project_ctx.as_deref(),
                )
                .await?;
                // Inject role as system prompt extension (keyed, additive)
                if let Some(role) = &role {
                    inject_team_role(&runner, role).await;
                }
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

            process_agent_communications(self.ctx.team, ctx, &session_key, &profile_name, &output);

            ctx.work_items().set_output(step_id, &output.response)?;

            results.push((profile_name, output.response));
        }

        // Merge
        match fan_out_config.merge_strategy {
            MergeStrategy::Concatenate => Ok(merge_concatenate(&results)),
            MergeStrategy::Summary => {
                let broadcast_section = format_broadcast_context(ctx, "**Team broadcasts:**");
                let summary_input = build_summary_input(input, &results, &broadcast_section);

                let first_profile = resolve_profile(
                    self.ctx.profile_store,
                    &self.ctx.team.agents[0].profile,
                    self.ctx.model_override,
                )?;

                let project = ctx.project_context.as_deref();
                let runner =
                    get_or_create(self.ctx.pool, &first_profile, &session_key, project).await?;
                let output = runner.run(&summary_input).await?;
                Ok(output.response)
            }
        }
    }
}

/// Merge results by concatenating each agent's output with headers and separators.
pub(crate) fn merge_concatenate(results: &[(String, String)]) -> String {
    results
        .iter()
        .map(|(name, output)| format!("## {name}\n\n{output}"))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

/// Build the summary prompt for the synthesizer agent.
pub(crate) fn build_summary_input(
    input: &str,
    results: &[(String, String)],
    broadcast_section: &str,
) -> String {
    let combined = results
        .iter()
        .map(|(name, output)| format!("### {name}\n{output}"))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "Multiple agents investigated the following question:\n\n\
         **Original question:** {input}\n\n\
         **Agent results:**\n\n{combined}{broadcast_section}\n\n\
         Please synthesize these results into a single coherent response."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_concatenate_empty() {
        let result = merge_concatenate(&[]);
        assert_eq!(result, "");
    }

    #[test]
    fn test_merge_concatenate_multiple() {
        let results = vec![
            ("coder".to_string(), "Fixed the bug.".to_string()),
            ("reviewer".to_string(), "LGTM.".to_string()),
        ];
        let merged = merge_concatenate(&results);
        assert!(merged.contains("## coder\n\nFixed the bug."));
        assert!(merged.contains("---"));
        assert!(merged.contains("## reviewer\n\nLGTM."));
    }

    #[test]
    fn test_build_summary_input() {
        let results = vec![
            ("agent1".to_string(), "result1".to_string()),
            ("agent2".to_string(), "result2".to_string()),
        ];
        let summary = build_summary_input("what is rust?", &results, "");
        assert!(summary.contains("**Original question:** what is rust?"));
        assert!(summary.contains("### agent1\nresult1"));
        assert!(summary.contains("### agent2\nresult2"));
        assert!(summary.contains("Please synthesize"));
    }
}
