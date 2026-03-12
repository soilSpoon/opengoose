use opengoose_persistence::MessageType;
use tracing::{info, warn};

use crate::context::OrchestrationContext;
use crate::runner::AgentOutput;
use crate::team::TeamDefinition;

pub(crate) fn is_team_member(team: &TeamDefinition, agent_name: &str) -> bool {
    team.agents.iter().any(|a| a.profile == agent_name)
}

pub(crate) fn process_agent_communications(
    team: &TeamDefinition,
    ctx: &OrchestrationContext,
    session_key: &str,
    agent_name: &str,
    output: &AgentOutput,
) {
    for broadcast in &output.broadcasts {
        ctx.broadcast(agent_name, broadcast);
    }
    enqueue_validated_delegations(team, ctx, session_key, agent_name, &output.delegations);
}

fn enqueue_validated_delegations(
    team: &TeamDefinition,
    ctx: &OrchestrationContext,
    session_key: &str,
    sender: &str,
    delegations: &[(String, String)],
) {
    for (recipient, msg) in delegations {
        if recipient == sender {
            info!(
                %sender,
                "self-delegation rejected (would cause cycle)"
            );
            continue;
        }
        if is_team_member(team, recipient) {
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
