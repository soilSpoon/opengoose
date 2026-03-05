use anyhow::{anyhow, Result};
use tracing::{info, warn};

use opengoose_profiles::ProfileStore;

use crate::agent_pool::AgentPool;
use crate::context::OrchestrationContext;
use crate::orchestrator::process_agent_communications;
use crate::prompt_context::{build_role_context, load_history_pairs};
use crate::runner::AgentRunner;
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

        // Use structured JSON output for reliable classification instead of
        // fragile digit-parsing. This leverages the LLM's ability to produce
        // valid JSON reliably.
        let classify_input = format!(
            "You are a message router. Given the user's message, pick the SINGLE best agent \
             to handle it.\n\n\
             Available agents:\n{agent_list}\n\n\
             User message: {input}\n\n\
             Respond with ONLY a JSON object in this exact format (no other text):\n\
             {{\"agent\": <number>, \"reason\": \"<brief reason>\"}}\n\
             where <number> is the 0-indexed agent number."
        );

        let classifier = AgentRunner::from_inline_prompt(
            "You are a classification assistant. Always respond with valid JSON only, no markdown fences.",
            "router-classifier",
        )
        .await?;
        let classification = classifier.run(&classify_input).await?;

        let raw = classification.response.trim().to_string();
        let chosen_idx = parse_router_json(&raw, self.team.agents.len());

        info!(
            raw_classification = %raw,
            chosen_idx,
            "router classified"
        );

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

        let role_ctx = build_role_context(chosen_agent.role.as_deref(), "Your role");

        let runner = self.pool.get_or_create(&profile).await?;

        // Seed conversation history into the Goose session.
        let history_pairs = load_history_pairs(ctx);
        if !history_pairs.is_empty() {
            if let Err(e) = runner.seed_history(&history_pairs).await {
                warn!("failed to seed history for routed agent: {e}");
            }
        }

        let final_input = format!("{input}{role_ctx}");

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

/// Parse the router's JSON classification response.
///
/// Expects `{"agent": N, ...}` but falls back to digit-parsing for robustness.
fn parse_router_json(raw: &str, agent_count: usize) -> usize {
    // Try JSON parse first
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(n) = v.get("agent").and_then(|a| a.as_u64()) {
            return (n as usize).min(agent_count.saturating_sub(1));
        }
    }

    // Strip markdown fences if present and retry
    let stripped = raw
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(stripped) {
        if let Some(n) = v.get("agent").and_then(|a| a.as_u64()) {
            return (n as usize).min(agent_count.saturating_sub(1));
        }
    }

    // Fallback: extract first digit (legacy behavior)
    warn!(response = %raw, "router JSON parse failed, falling back to digit extraction");
    raw.split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0)
        .min(agent_count.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_response() {
        assert_eq!(parse_router_json(r#"{"agent": 2, "reason": "code task"}"#, 3), 2);
    }

    #[test]
    fn parse_json_with_markdown_fences() {
        assert_eq!(
            parse_router_json("```json\n{\"agent\": 1, \"reason\": \"research\"}\n```", 3),
            1
        );
    }

    #[test]
    fn parse_clamps_to_max() {
        assert_eq!(parse_router_json(r#"{"agent": 99}"#, 3), 2);
    }

    #[test]
    fn parse_fallback_digit() {
        assert_eq!(parse_router_json("I think agent 1 is best", 3), 1);
    }

    #[test]
    fn parse_fallback_default() {
        assert_eq!(parse_router_json("no numbers here", 3), 0);
    }
}
