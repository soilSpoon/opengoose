use anyhow::{anyhow, Result};
use tokio::task::JoinSet;
use tracing::{debug, info};

use opengoose_profiles::ProfileStore;

use crate::runner::AgentRunner;
use crate::team::{MergeStrategy, TeamDefinition, Workflow};

/// Executes a team workflow by orchestrating multiple agent runners.
pub struct TeamOrchestrator {
    team: TeamDefinition,
    profile_store: ProfileStore,
}

impl TeamOrchestrator {
    pub fn new(team: TeamDefinition, profile_store: ProfileStore) -> Self {
        Self {
            team,
            profile_store,
        }
    }

    /// Execute the team's workflow with the given input and return the final output.
    pub async fn execute(&self, input: &str) -> Result<String> {
        info!(team = %self.team.name(), workflow = ?self.team.workflow, "executing team");

        match self.team.workflow {
            Workflow::Chain => self.execute_chain(input).await,
            Workflow::FanOut => self.execute_fan_out(input).await,
            Workflow::Router => self.execute_router(input).await,
        }
    }

    /// Chain: run agents sequentially, piping output from one to the next.
    async fn execute_chain(&self, input: &str) -> Result<String> {
        let mut current = input.to_string();

        for (i, team_agent) in self.team.agents.iter().enumerate() {
            let profile = self.profile_store.get(&team_agent.profile).map_err(|_| {
                anyhow!("profile `{}` not found", team_agent.profile)
            })?;

            let role_ctx = team_agent
                .role
                .as_deref()
                .map(|r| format!("\n\n[Your role in this team: {}]", r))
                .unwrap_or_default();

            let runner = AgentRunner::from_profile(&profile).await?;

            let step_input = if i == 0 {
                format!("{current}{role_ctx}")
            } else {
                format!(
                    "Previous agent's output:\n---\n{current}\n---\n\
                     Please continue based on the above.{role_ctx}"
                )
            };

            debug!(
                step = i,
                profile = %team_agent.profile,
                "chain step"
            );

            current = runner.run(&step_input).await?;
        }

        Ok(current)
    }

    /// Fan-out: run all agents in parallel, then merge results.
    async fn execute_fan_out(&self, input: &str) -> Result<String> {
        let fan_out_config = self
            .team
            .fan_out
            .as_ref()
            .ok_or_else(|| anyhow!("fan-out workflow requires fan_out config"))?;

        let mut join_set = JoinSet::new();

        for team_agent in &self.team.agents {
            let profile = self.profile_store.get(&team_agent.profile).map_err(|_| {
                anyhow!("profile `{}` not found", team_agent.profile)
            })?;

            let role_ctx = team_agent
                .role
                .as_deref()
                .map(|r| format!("\n\n[Your role: {}]", r))
                .unwrap_or_default();

            let agent_input = format!("{input}{role_ctx}");
            let profile_name = team_agent.profile.clone();

            join_set.spawn(async move {
                let runner = AgentRunner::from_profile(&profile).await?;
                let result = runner.run(&agent_input).await?;
                Ok::<(String, String), anyhow::Error>((profile_name, result))
            });
        }

        // Collect results
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let (profile_name, output) = result??;
            debug!(profile = %profile_name, "fan-out agent complete");
            results.push((profile_name, output));
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
                // Use a summarizer agent to synthesize all results
                let combined = results
                    .iter()
                    .map(|(name, output)| format!("### {name}\n{output}"))
                    .collect::<Vec<_>>()
                    .join("\n\n");

                let summary_input = format!(
                    "Multiple agents investigated the following question:\n\n\
                     **Original question:** {input}\n\n\
                     **Agent results:**\n\n{combined}\n\n\
                     Please synthesize these results into a single coherent response."
                );

                // Use the first agent's profile for summarization
                let first_profile = self
                    .profile_store
                    .get(&self.team.agents[0].profile)
                    .map_err(|_| anyhow!("profile not found for summarizer"))?;

                let runner = AgentRunner::from_profile(&first_profile).await?;
                runner.run(&summary_input).await
            }
        }
    }

    /// Router: classify the input and dispatch to the best-matching agent.
    async fn execute_router(&self, input: &str) -> Result<String> {
        let _router_config = self
            .team
            .router
            .as_ref()
            .ok_or_else(|| anyhow!("router workflow requires router config"))?;

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

        // Use a lightweight classification prompt
        let classify_input = format!(
            "You are a message router. Given the user's message, pick the SINGLE best agent \
             to handle it. Reply with ONLY the agent number (0-indexed).\n\n\
             Available agents:\n{agent_list}\n\n\
             User message: {input}\n\n\
             Best agent number:"
        );

        // Use the first agent to do classification (cheap, fast)
        let first_profile = self
            .profile_store
            .get(&self.team.agents[0].profile)
            .map_err(|_| anyhow!("profile not found for router"))?;

        let classifier = AgentRunner::from_profile(&first_profile).await?;
        let classification = classifier.run(&classify_input).await?;

        // Parse the agent index from classification response
        let chosen_idx = classification
            .trim()
            .chars()
            .find(|c| c.is_ascii_digit())
            .and_then(|c| c.to_digit(10))
            .map(|d| d as usize)
            .unwrap_or(0);

        let chosen_idx = chosen_idx.min(self.team.agents.len() - 1);
        let chosen_agent = &self.team.agents[chosen_idx];

        info!(
            chosen = %chosen_agent.profile,
            index = chosen_idx,
            "router dispatching"
        );

        let profile = self.profile_store.get(&chosen_agent.profile).map_err(|_| {
            anyhow!("profile `{}` not found", chosen_agent.profile)
        })?;

        let role_ctx = chosen_agent
            .role
            .as_deref()
            .map(|r| format!("\n\n[Your role: {}]", r))
            .unwrap_or_default();

        let runner = AgentRunner::from_profile(&profile).await?;
        runner.run(&format!("{input}{role_ctx}")).await
    }
}
