use std::collections::HashMap;

use anyhow::{Result, anyhow};
use tracing::{info, warn};

use opengoose_profiles::ProfileStore;

use crate::chain_executor::{get_or_create, load_history_pairs};
use crate::context::OrchestrationContext;
use crate::orchestrator::process_agent_communications;
use crate::runner::AgentRunner;
use crate::team::TeamDefinition;

/// Executes the Router workflow: classifies the input and dispatches
/// to the best-matching agent.
pub struct RouterExecutor<'a> {
    team: &'a TeamDefinition,
    profile_store: &'a ProfileStore,
    pool: &'a mut HashMap<String, AgentRunner>,
}

impl<'a> RouterExecutor<'a> {
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
        let _router_config = self
            .team
            .router
            .as_ref()
            .ok_or_else(|| anyhow!("router workflow requires router config"))?;

        let session_key = ctx.session_key.to_stable_id();

        let agent_list = build_agent_list(&self.team.agents);
        let classify_input = build_classify_prompt(&agent_list, input);

        let classifier = AgentRunner::from_inline_prompt(
            "You are a classification assistant. You MUST use the final_output tool to report your answer.",
            "router-classifier",
        )
        .await?;

        // Use Goose's FinalOutputTool with a JSON schema to guarantee structured output.
        let schema = router_response_schema();
        classifier.set_response_schema(schema).await;

        let raw = classifier.run_structured(&classify_input).await?;
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

        let runner = get_or_create(self.pool, &profile).await?;

        // Inject role as system prompt extension (keyed, additive)
        if let Some(role) = &chosen_agent.role {
            runner
                .extend_system_prompt("team_role", &format!("Your role: {role}"))
                .await;
        }

        let history_pairs = load_history_pairs(ctx);
        if !history_pairs.is_empty()
            && let Err(e) = runner.seed_history(&history_pairs).await
        {
            warn!("failed to seed history for routed agent: {e}");
        }

        let final_input = input.to_string();

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
                ctx.work_items().set_error(step_id, &e.to_string())?;
                Err(e)
            }
        }
    }
}

/// Build a formatted agent list for the router classification prompt.
pub(crate) fn build_agent_list(agents: &[crate::team::TeamAgent]) -> String {
    agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let role = a.role.as_deref().unwrap_or("general");
            format!("{i}. {profile} — {role}", profile = a.profile)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build the classification prompt for the router.
pub(crate) fn build_classify_prompt(agent_list: &str, input: &str) -> String {
    format!(
        "You are a message router. Given the user's message, pick the SINGLE best agent \
         to handle it.\n\n\
         Available agents:\n{agent_list}\n\n\
         User message: {input}\n\n\
         Pick the best agent and use the final_output tool to report your choice."
    )
}

/// Build the JSON schema for the router's structured response.
pub(crate) fn router_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "agent": {
                "type": "integer",
                "description": "0-indexed agent number"
            },
            "reason": {
                "type": "string",
                "description": "Brief reason for the choice"
            }
        },
        "required": ["agent", "reason"]
    })
}

/// Parse the router's JSON classification response.
fn parse_router_json(raw: &str, agent_count: usize) -> usize {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw)
        && let Some(n) = v.get("agent").and_then(|a| a.as_u64())
    {
        return (n as usize).min(agent_count.saturating_sub(1));
    }

    // Strip markdown fences if present and retry
    let stripped = raw
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(stripped)
        && let Some(n) = v.get("agent").and_then(|a| a.as_u64())
    {
        return (n as usize).min(agent_count.saturating_sub(1));
    }

    // Fallback: extract first digit
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
        assert_eq!(
            parse_router_json(r#"{"agent": 2, "reason": "code task"}"#, 3),
            2
        );
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

    #[test]
    fn test_build_agent_list() {
        use crate::team::TeamAgent;
        let agents = vec![
            TeamAgent {
                profile: "developer".into(),
                role: Some("write code".into()),
            },
            TeamAgent {
                profile: "reviewer".into(),
                role: None,
            },
        ];
        let list = build_agent_list(&agents);
        assert_eq!(list, "0. developer — write code\n1. reviewer — general");
    }

    #[test]
    fn test_build_classify_prompt() {
        let prompt = build_classify_prompt("0. coder — code\n1. reviewer — review", "fix bug");
        assert!(prompt.contains("You are a message router"));
        assert!(prompt.contains("0. coder — code"));
        assert!(prompt.contains("User message: fix bug"));
    }

    #[test]
    fn test_router_response_schema() {
        let schema = router_response_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["agent"].is_object());
        assert!(schema["properties"]["reason"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("agent")));
        assert!(required.contains(&serde_json::json!("reason")));
    }

    #[test]
    fn parse_json_with_fences_no_json_label() {
        assert_eq!(
            parse_router_json("```\n{\"agent\": 0, \"reason\": \"test\"}\n```", 3),
            0
        );
    }

    #[test]
    fn parse_json_with_zero_agents() {
        // Edge case: agent_count = 0 → saturating_sub(1) = 0
        assert_eq!(parse_router_json(r#"{"agent": 5}"#, 0), 0);
    }

    #[test]
    fn parse_json_single_agent() {
        assert_eq!(parse_router_json(r#"{"agent": 0}"#, 1), 0);
    }

    #[test]
    fn test_build_agent_list_empty() {
        let list = build_agent_list(&[]);
        assert_eq!(list, "");
    }

    #[test]
    fn test_build_agent_list_single() {
        use crate::team::TeamAgent;
        let agents = vec![TeamAgent {
            profile: "solo".into(),
            role: Some("do everything".into()),
        }];
        let list = build_agent_list(&agents);
        assert_eq!(list, "0. solo — do everything");
    }
}
