//! Slack Socket Mode envelope classification.

use crate::types::{EventCallback, SocketEnvelope};
use opengoose_types::{Platform, SessionKey};

use super::types::SlackEnvelopeAction;

/// Classify a Socket Mode envelope into an action to take.
pub(in crate::gateway) fn classify_slack_envelope(
    envelope: &SocketEnvelope,
    bot_user_id: &str,
) -> SlackEnvelopeAction {
    let Some(payload) = envelope.payload.as_ref() else {
        return SlackEnvelopeAction::Ignore;
    };

    match envelope.envelope_type.as_str() {
        "events_api" => match serde_json::from_value::<EventCallback>(payload.clone()) {
            Ok(callback) => {
                let Some(event) = callback.event else {
                    return SlackEnvelopeAction::Ignore;
                };

                if event.event_type != "message" || event.subtype.is_some() {
                    return SlackEnvelopeAction::Ignore;
                }

                if event.bot_id.is_some() {
                    return SlackEnvelopeAction::Ignore;
                }

                if event.user.as_deref() == Some(bot_user_id) {
                    return SlackEnvelopeAction::Ignore;
                }

                let Some(channel) = event.channel.as_deref() else {
                    return SlackEnvelopeAction::Ignore;
                };
                let Some(text) = event.text.as_deref().map(str::trim) else {
                    return SlackEnvelopeAction::Ignore;
                };

                if text.is_empty() {
                    return SlackEnvelopeAction::Ignore;
                }

                let Some(display_name) = event.user else {
                    return SlackEnvelopeAction::Ignore;
                };
                let team_id = callback.team_id.as_deref().unwrap_or("unknown");
                SlackEnvelopeAction::Relay {
                    session_key: SessionKey::new(Platform::Slack, team_id, channel),
                    channel: channel.to_string(),
                    text: text.to_string(),
                    display_name,
                }
            }
            Err(_) => SlackEnvelopeAction::Ignore,
        },
        "slash_commands" => {
            match serde_json::from_value::<crate::types::SlashCommand>(payload.clone()) {
                Ok(cmd) if cmd.command.as_deref() == Some("/team") => {
                    SlackEnvelopeAction::TeamCommand(cmd)
                }
                _ => SlackEnvelopeAction::Ignore,
            }
        }
        _ => SlackEnvelopeAction::Ignore,
    }
}

#[cfg(test)]
mod tests;
