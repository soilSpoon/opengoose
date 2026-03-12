use serde_json::Value;

use crate::types::SlashCommand;

use super::super::types::SlackEnvelopeAction;

const TEAM_COMMAND: &str = "/team";

pub(super) fn classify_slash_command(payload: &Value) -> SlackEnvelopeAction {
    match serde_json::from_value::<SlashCommand>(payload.clone()) {
        Ok(cmd) if is_team_command(&cmd) => SlackEnvelopeAction::TeamCommand(cmd),
        _ => SlackEnvelopeAction::Ignore,
    }
}

fn is_team_command(cmd: &SlashCommand) -> bool {
    cmd.command.as_deref() == Some(TEAM_COMMAND)
}
