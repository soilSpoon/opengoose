use anyhow::{anyhow, Result};
use tracing::{info, warn};

use opengoose_profiles::ProfileStore;

use crate::agent_pool::AgentPool;
use crate::context::OrchestrationContext;
use crate::orchestrator::process_agent_communications;
use crate::prompt_context::PromptContextBuilder;
use crate::team::TeamDefinition;

/// Executes the Router workflow: classifies the input and dispatches
/// to the best-matching agent.
pub struct RouterExecutor<'a> {
    team: &'a TeamDefinition,
    profile_store: &'a ProfileStore,
    pool: &'a mut AgentPool,
}

impl<'a> RouterExecutor<'a> {
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
        let _router_config = self
            .team
            .router
            .as_ref()
            .ok_or_else(|| anyhow!("router workflow requires router config"))?;

        let session_key = ctx.session_key.to_stable_id();

        // Build agent descriptions for classification
        let agent_list = self
            .team
            .agents
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let role = a.role.as_deref().unwrap_or("general");
                format!("{i}. {profile} — {role}", profile = a.profile)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let classify_input = format!(
            "You are a message router. Given the user's message, pick the SINGLE best agent \
             to handle it. Reply with ONLY the agent number (0-indexed).\n\n\
             Available agents:\n{agent_list}\n\n\
             User message: {input}\n\n\
             Best agent number:"
        );

        let first_profile = self
            .profile_store
            .get(&self.team.agents[0].profile)
            .map_err(|_| anyhow!("profile `{}` not found", self.team.agents[0].profile))?;

        let classifier = self.pool.get_or_create(&first_profile).await?;
        let classification = classifier.run(&classify_input).await?;

        let raw_classification = classification.response.trim().to_string();
        let chosen_idx = raw_classification
            .split(|c: char| !c.is_ascii_digit())
            .find(|s| !s.is_empty())
            .and_then(|s| s.parse::<usize>().ok());

        if chosen_idx.is_none() {
            warn!(
                response = %raw_classification,
                "router classifier returned no digit, defaulting to agent 0"
            );
        }

        let chosen_idx = chosen_idx.unwrap_or(0).min(self.team.agents.len() - 1);
        let chosen_agent = &self.team.agents[chosen_idx];

        info!(
            chosen = %chosen_agent.profile,
            index = chosen_idx,
            "router dispatching"
        );

        // Create work item for the chosen agent
        let step_id = ctx.work_items().create(
            &session_key,
            &ctx.team_run_id,
            &format!("Router → {}", chosen_agent.profile),
            Some(parent_id),
        )?;
        ctx.work_items()
            .assign(step_id, &chosen_agent.profile, Some(chosen_idx as i32))?;

        let profile = self
            .profile_store
            .get(&chosen_agent.profile)
            .map_err(|_| anyhow!("profile `{}` not found", chosen_agent.profile))?;

        let prompt_ctx = PromptContextBuilder::new(ctx, chosen_agent.role.as_deref(), "Your role", "");

        let runner = self.pool.get_or_create(&profile).await?;
        let final_input = format!(
            "{}{}{}",
            prompt_ctx.history_prefix(),
            input,
            prompt_ctx.role_ctx()
        );

        match runner.run(&final_input).await {
            Ok(output) => {
                process_agent_communications(
                    self.team,
                    ctx,
                    &session_key,
                    &chosen_agent.profile,
                    &output,
                );
                ctx.work_items().set_output(step_id, &output.response)?;
                Ok(output.response)
            }
            Err(e) => {
                ctx.work_items()
                    .set_error(step_id, &e.to_string())?;
                Err(e)
            }
        }
    }
}
