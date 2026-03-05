use anyhow::{anyhow, Result};
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use opengoose_persistence::{MessageType, WorkStatus};
use opengoose_profiles::{AgentProfile, ProfileStore};

/// Maximum delegation recursion depth to prevent infinite loops.
const MAX_DELEGATION_DEPTH: usize = 3;

/// Aggregated outcome of delegation processing.
#[derive(Debug, Default)]
struct DelegationOutcome {
    succeeded: usize,
    failed: usize,
}

use crate::context::OrchestrationContext;
use crate::runner::AgentRunner;
use crate::team::{MergeStrategy, TeamDefinition, Workflow};

/// Executes a team workflow by orchestrating multiple agent runners.
///
/// All execution goes through `OrchestrationContext`, which provides
/// message queue, work item tracking, and broadcast log access.
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

    /// Execute the team's workflow with full orchestration context.
    pub async fn execute(&self, input: &str, ctx: &OrchestrationContext) -> Result<String> {
        info!(team = %self.team.name(), workflow = ?self.team.workflow, "executing team");

        let session_key = ctx.session_key.to_stable_id();
        let workflow_str = match self.team.workflow {
            Workflow::Chain => "chain",
            Workflow::FanOut => "fan_out",
            Workflow::Router => "router",
        };

        // Create orchestration run for crash recovery
        ctx.orchestration().create_run(
            &ctx.team_run_id,
            &session_key,
            self.team.name(),
            workflow_str,
            input,
            self.team.agents.len() as i32,
        )?;

        // Create parent work item and persist the original input for resume
        let parent_id = ctx.work_items().create(
            &session_key,
            &ctx.team_run_id,
            &format!("Team: {}", self.team.name()),
            None,
        )?;
        ctx.work_items().set_input(&parent_id, input)?;
        ctx.work_items()
            .update_status(&parent_id, WorkStatus::InProgress)?;

        let result = match self.team.workflow {
            Workflow::Chain => self.execute_chain(input, ctx, &parent_id).await,
            Workflow::FanOut => self.execute_fan_out(input, ctx, &parent_id).await,
            Workflow::Router => self.execute_router(input, ctx, &parent_id).await,
        };

        // Process pending delegations after the main workflow succeeds
        if result.is_ok() {
            match self
                .process_pending_delegations(ctx, &parent_id, 0)
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

        // Build final response, appending dead-letter report if any
        let dead = match ctx.queue().get_dead_letters(&ctx.team_run_id) {
            Ok(v) => v,
            Err(e) => {
                warn!("failed to retrieve dead letters for run {}: {e}", ctx.team_run_id);
                Default::default()
            }
        };

        match &result {
            Ok(response) => {
                ctx.work_items().set_output(&parent_id, response)?;
                ctx.orchestration()
                    .complete_run(&ctx.team_run_id, response)?;
            }
            Err(e) => {
                let err_msg = e.to_string();
                ctx.work_items().set_error(&parent_id, &err_msg)?;
                ctx.orchestration()
                    .fail_run(&ctx.team_run_id, &err_msg)?;
            }
        }

        let mut final_response = result?;
        if !dead.is_empty() {
            let notes = dead
                .iter()
                .map(|d| {
                    format!(
                        "- {} → {}: {}",
                        d.sender,
                        d.recipient,
                        d.error.as_deref().unwrap_or("unknown error")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            final_response
                .push_str(&format!("\n\n---\n**Failed delegations:**\n{notes}"));
        }

        Ok(final_response)
    }

    /// Resume a suspended chain workflow from where it left off.
    pub async fn resume(
        &self,
        ctx: &OrchestrationContext,
        parent_work_id: &str,
    ) -> Result<String> {
        if self.team.workflow != Workflow::Chain {
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
                // No completed steps — restart from step 0 with the run's original input.
                let parent = ctx.work_items().get(parent_work_id)?.ok_or_else(|| {
                    anyhow!("parent work item {parent_work_id} not found")
                })?;
                let original_input = parent.input.unwrap_or_default();
                (0, original_input)
            }
        };

        // Update orchestration run status back to running
        ctx.orchestration().resume_run(&ctx.team_run_id)?;
        ctx.orchestration()
            .advance_step(&ctx.team_run_id, start_step)?;

        let result = self
            .execute_chain_from_step(&last_output, ctx, parent_work_id, start_step as usize)
            .await;

        // Update run and work item status (mirrors execute() logic)
        match &result {
            Ok(response) => {
                ctx.work_items().set_output(parent_work_id, response)?;
                ctx.orchestration()
                    .complete_run(&ctx.team_run_id, response)?;
            }
            Err(e) => {
                let err_msg = e.to_string();
                ctx.work_items().set_error(parent_work_id, &err_msg)?;
                ctx.orchestration()
                    .fail_run(&ctx.team_run_id, &err_msg)?;
            }
        }

        result
    }

    /// Chain: run agents sequentially, piping output from one to the next.
    async fn execute_chain(
        &self,
        input: &str,
        ctx: &OrchestrationContext,
        parent_id: &str,
    ) -> Result<String> {
        self.execute_chain_from_step(input, ctx, parent_id, 0).await
    }

    async fn execute_chain_from_step(
        &self,
        input: &str,
        ctx: &OrchestrationContext,
        parent_id: &str,
        start_step: usize,
    ) -> Result<String> {
        let mut current = input.to_string();
        let session_key = ctx.session_key.to_stable_id();
        let history_text = Self::load_and_format_history(ctx);

        for (i, team_agent) in self.team.agents.iter().enumerate().skip(start_step) {
            let profile = self.load_agent_profile(&team_agent.profile)?;
            let role_ctx =
                Self::build_role_context(team_agent.role.as_deref(), "Your role in this team");
            let broadcast_ctx = Self::format_broadcast_context(ctx, "Team findings so far");

            // Create work item for this step
            let step_id = ctx.work_items().create(
                &session_key,
                &ctx.team_run_id,
                &format!("Step {i}: {}", team_agent.profile),
                Some(parent_id),
            )?;
            ctx.work_items()
                .assign(&step_id, &team_agent.profile, Some(i as i32))?;
            ctx.work_items().set_input(&step_id, &current)?;

            let runner = AgentRunner::from_profile(&profile).await?;

            let step_input = if i == start_step && start_step == 0 {
                let history_prefix = Self::build_history_prefix(&history_text);
                format!("{history_prefix}{current}{role_ctx}{broadcast_ctx}")
            } else {
                format!(
                    "Previous agent's output:\n---\n{current}\n---\n\
                     Please continue based on the above.{role_ctx}{broadcast_ctx}"
                )
            };

            debug!(step = i, profile = %team_agent.profile, "chain step");

            ctx.orchestration()
                .advance_step(&ctx.team_run_id, i as i32)?;

            match runner.run(&step_input).await {
                Ok(output) => {
                    self.process_agent_communications(
                        ctx,
                        &session_key,
                        &team_agent.profile,
                        &output,
                    );
                    ctx.work_items().set_output(&step_id, &output.response)?;
                    current = output.response;
                }
                Err(e) => {
                    ctx.work_items()
                        .set_error(&step_id, &e.to_string())?;
                    return Err(e);
                }
            }
        }

        Ok(current)
    }

    /// Fan-out: run all agents in parallel, then merge results.
    ///
    /// Note: Agents in fan-out cannot see each other's broadcasts in real-time.
    /// Broadcasts become visible to the summary merge step and to any
    /// subsequent chain steps if this fan-out is part of a larger workflow.
    async fn execute_fan_out(
        &self,
        input: &str,
        ctx: &OrchestrationContext,
        parent_id: &str,
    ) -> Result<String> {
        let fan_out_config = self
            .team
            .fan_out
            .as_ref()
            .ok_or_else(|| anyhow!("fan-out workflow requires fan_out config"))?;

        let session_key = ctx.session_key.to_stable_id();
        let team_run_id = ctx.team_run_id.clone();
        let history_text = Self::load_and_format_history(ctx);
        let history_prefix = Self::build_history_prefix(&history_text);

        let mut join_set = JoinSet::new();

        for (i, team_agent) in self.team.agents.iter().enumerate() {
            let profile = self.load_agent_profile(&team_agent.profile)?;

            // Create work item
            let step_id = ctx.work_items().create(
                &session_key,
                &team_run_id,
                &format!("Fan-out: {}", team_agent.profile),
                Some(parent_id),
            )?;
            ctx.work_items()
                .assign(&step_id, &team_agent.profile, Some(i as i32))?;

            let role_ctx = Self::build_role_context(team_agent.role.as_deref(), "Your role");

            let agent_input = format!(
                "{history_prefix}{input}{role_ctx}\n\n\
                 [You are part of a parallel team. If you make important discoveries, \
                 prefix them with [BROADCAST]: so other agents can see them.]"
            );
            let profile_name = team_agent.profile.clone();

            join_set.spawn(async move {
                let runner = AgentRunner::from_profile(&profile).await?;
                let output = runner.run(&agent_input).await?;
                Ok::<(String, String, crate::runner::AgentOutput), anyhow::Error>((
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

            self.process_agent_communications(ctx, &session_key, &profile_name, &output);

            ctx.work_items()
                .set_output(&step_id, &output.response)?;

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

                let broadcast_section =
                    Self::format_broadcast_context(ctx, "**Team broadcasts:**");

                let summary_input = format!(
                    "Multiple agents investigated the following question:\n\n\
                     **Original question:** {input}\n\n\
                     **Agent results:**\n\n{combined}{broadcast_section}\n\n\
                     Please synthesize these results into a single coherent response."
                );

                let first_profile = self.load_agent_profile(&self.team.agents[0].profile)?;

                let runner = AgentRunner::from_profile(&first_profile).await?;
                let output = runner.run(&summary_input).await?;
                Ok(output.response)
            }
        }
    }

    /// Router: classify the input and dispatch to the best-matching agent.
    async fn execute_router(
        &self,
        input: &str,
        ctx: &OrchestrationContext,
        parent_id: &str,
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

        let first_profile = self.load_agent_profile(&self.team.agents[0].profile)?;

        let classifier = AgentRunner::from_profile(&first_profile).await?;
        let classification = classifier.run(&classify_input).await?;

        let chosen_idx = classification
            .response
            .trim()
            .split(|c: char| !c.is_ascii_digit())
            .find(|s| !s.is_empty())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let chosen_idx = chosen_idx.min(self.team.agents.len() - 1);
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
            .assign(&step_id, &chosen_agent.profile, Some(chosen_idx as i32))?;

        let profile = self.load_agent_profile(&chosen_agent.profile)?;
        let role_ctx = Self::build_role_context(chosen_agent.role.as_deref(), "Your role");
        let history_text = Self::load_and_format_history(ctx);
        let history_prefix = Self::build_history_prefix(&history_text);

        let runner = AgentRunner::from_profile(&profile).await?;
        let final_input = format!("{history_prefix}{input}{role_ctx}");

        match runner.run(&final_input).await {
            Ok(output) => {
                self.process_agent_communications(
                    ctx,
                    &session_key,
                    &chosen_agent.profile,
                    &output,
                );
                ctx.work_items().set_output(&step_id, &output.response)?;
                Ok(output.response)
            }
            Err(e) => {
                ctx.work_items()
                    .set_error(&step_id, &e.to_string())?;
                Err(e)
            }
        }
    }

    // ── Shared helpers ─────────────────────────────────────────────

    /// Load an agent profile by name, returning a descriptive error on failure.
    fn load_agent_profile(&self, profile_name: &str) -> Result<AgentProfile> {
        self.profile_store
            .get(profile_name)
            .map_err(|_| anyhow!("profile `{}` not found", profile_name))
    }

    /// Build the role context suffix from an optional role description.
    fn build_role_context(role: Option<&str>, label: &str) -> String {
        role.map(|r| format!("\n\n[{label}: {r}]"))
            .unwrap_or_default()
    }

    /// Load conversation history and format it as a text block.
    fn load_and_format_history(ctx: &OrchestrationContext) -> String {
        let history = match ctx.sessions().load_history(&ctx.session_key, 20) {
            Ok(v) => v,
            Err(e) => {
                warn!("failed to load conversation history: {e}");
                Default::default()
            }
        };
        format_history(&history)
    }

    /// Build a history prefix string for prepending to agent input.
    fn build_history_prefix(history_text: &str) -> String {
        if history_text.is_empty() {
            String::new()
        } else {
            format!("Conversation history:\n---\n{history_text}\n---\n\nCurrent message: ")
        }
    }

    /// Format broadcasts into a context section for agent input.
    fn format_broadcast_context(ctx: &OrchestrationContext, header: &str) -> String {
        let broadcasts = ctx.read_broadcasts(None);
        if broadcasts.is_empty() {
            String::new()
        } else {
            let text: String = broadcasts
                .iter()
                .map(|b| format!("- [{}]: {}", b.sender, b.content))
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n\n[{header}]:\n{text}")
        }
    }

    /// Record broadcasts and enqueue validated delegations from agent output.
    fn process_agent_communications(
        &self,
        ctx: &OrchestrationContext,
        session_key: &str,
        agent_name: &str,
        output: &crate::runner::AgentOutput,
    ) {
        for broadcast in &output.broadcasts {
            ctx.broadcast(agent_name, broadcast);
        }
        self.enqueue_validated_delegations(ctx, session_key, agent_name, &output.delegations);
    }

    // ── Delegation helpers ──────────────────────────────────────────

    /// Check if an agent name is a valid member of this team.
    fn is_team_member(&self, agent_name: &str) -> bool {
        self.team.agents.iter().any(|a| a.profile == agent_name)
    }

    /// Enqueue delegations after validating that each recipient is a team member.
    fn enqueue_validated_delegations(
        &self,
        ctx: &OrchestrationContext,
        session_key: &str,
        sender: &str,
        delegations: &[(String, String)],
    ) {
        for (recipient, msg) in delegations {
            if self.is_team_member(recipient) {
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

    /// Process all pending delegations for a run as a synchronous post-workflow step.
    ///
    /// Drains the delegation queue in a loop, executing each target agent.
    /// Supports recursive delegations up to `MAX_DELEGATION_DEPTH`.
    async fn process_pending_delegations(
        &self,
        ctx: &OrchestrationContext,
        parent_work_id: &str,
        depth: usize,
    ) -> Result<DelegationOutcome> {
        if depth >= MAX_DELEGATION_DEPTH {
            info!(depth, "max delegation depth reached, stopping");
            return Ok(DelegationOutcome::default());
        }

        let session_key = ctx.session_key.to_stable_id();
        let mut outcome = DelegationOutcome::default();

        // Dequeue all pending delegations once (bounded snapshot).
        // New delegations enqueued by these agents are handled by the
        // recursive call at the bottom, which increments depth.
        let delegations = ctx
            .queue()
            .dequeue_delegations(&ctx.team_run_id, 50)
            .map_err(|e| anyhow!("failed to dequeue delegations: {e}"))?;

        if delegations.is_empty() {
            return Ok(outcome);
        }

        for msg in delegations {
                // Recipient was already validated at enqueue time by
                // enqueue_validated_delegations(), so we proceed directly.

                // Create work item for this delegation
                let work_id = ctx.work_items().create(
                    &session_key,
                    &ctx.team_run_id,
                    &format!("Delegation: {} → {}", msg.sender, msg.recipient),
                    Some(parent_work_id),
                )?;
                ctx.work_items().assign(&work_id, &msg.recipient, None)?;
                ctx.work_items().set_input(&work_id, &msg.content)?;

                let profile = match self.profile_store.get(&msg.recipient) {
                    Ok(p) => p,
                    Err(_) => {
                        let err = format!("profile '{}' not found", msg.recipient);
                        ctx.work_items().set_error(&work_id, &err)?;
                        if let Err(e) = ctx.queue().fail(msg.id, &err) {
                            warn!("failed to mark delegation as failed: {e}");
                        }
                        outcome.failed += 1;
                        continue;
                    }
                };

                let delegation_input =
                    format!("[Delegated from {}]: {}", msg.sender, msg.content);

                info!(
                    sender = %msg.sender,
                    recipient = %msg.recipient,
                    depth,
                    "executing delegation"
                );

                match AgentRunner::from_profile(&profile).await {
                    Ok(runner) => match runner.run(&delegation_input).await {
                        Ok(output) => {
                            for broadcast in &output.broadcasts {
                                ctx.broadcast(&msg.recipient, broadcast);
                            }
                            self.enqueue_validated_delegations(
                                ctx,
                                &session_key,
                                &msg.recipient,
                                &output.delegations,
                            );
                            ctx.work_items().set_output(&work_id, &output.response)?;
                            if let Err(e) = ctx.queue().complete(msg.id) {
                                warn!("failed to mark delegation message as complete: {e}");
                            }
                            outcome.succeeded += 1;
                        }
                        Err(e) => {
                            ctx.work_items().set_error(&work_id, &e.to_string())?;
                            if let Err(qe) = ctx.queue().fail(msg.id, &e.to_string()) {
                                warn!("failed to mark delegation as failed: {qe}");
                            }
                            outcome.failed += 1;
                        }
                    },
                    Err(e) => {
                        ctx.work_items().set_error(&work_id, &e.to_string())?;
                        if let Err(qe) = ctx.queue().fail(msg.id, &e.to_string()) {
                            warn!("failed to mark delegation as failed: {qe}");
                        }
                        outcome.failed += 1;
                    }
                }
            }

        // Recurse for any delegations created by the delegated agents
        let sub = Box::pin(
            self.process_pending_delegations(ctx, parent_work_id, depth + 1),
        )
        .await?;
        outcome.succeeded += sub.succeeded;
        outcome.failed += sub.failed;

        Ok(outcome)
    }
}

/// Format conversation history into a text block for injection.
fn format_history(history: &[opengoose_persistence::HistoryMessage]) -> String {
    if history.is_empty() {
        return String::new();
    }
    history
        .iter()
        .map(|h| format!("[{}]: {}", h.role, h.content))
        .collect::<Vec<_>>()
        .join("\n")
}
