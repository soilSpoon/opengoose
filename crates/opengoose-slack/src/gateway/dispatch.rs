//! Slack envelope dispatch and `/team` command handling.

use tracing::{debug, error, info};

use opengoose_core::StreamResponder;
use opengoose_types::{Platform, SessionKey};

use crate::types::{SlashCommand, SocketEnvelope};

use super::envelope::classify_slack_envelope;
use super::types::SlackEnvelopeAction;
use super::{SLACK_MAX_LEN, SlackGateway};

impl SlackGateway {
    async fn handle_team_command(&self, cmd: &SlashCommand) {
        let Some(session_key) = team_command_session_key(cmd) else {
            return;
        };

        let response = self
            .bridge
            .handle_pairing(&session_key, team_command_args(cmd));

        if let Some(response_url) = cmd.response_url.as_deref() {
            self.respond_ephemeral(response_url, &response).await;
        }
    }

    /// Process a single Socket Mode envelope.
    pub(super) async fn handle_envelope(&self, envelope: &SocketEnvelope, bot_user_id: &str) {
        match classify_slack_envelope(envelope, bot_user_id) {
            SlackEnvelopeAction::Ignore => {
                debug!(envelope_type = %envelope.envelope_type, "ignoring slack envelope");
            }
            SlackEnvelopeAction::Relay {
                session_key,
                channel,
                text,
                display_name,
            } => {
                if !self.bridge.is_accepting_messages() {
                    info!(channel = %channel, "ignoring slack message during shutdown drain");
                    return;
                }
                debug!(
                    channel = %channel,
                    user = %display_name,
                    text_len = text.len(),
                    "relaying slack message to engine"
                );
                if let Err(error) = self
                    .bridge
                    .relay_and_drive_stream(
                        &session_key,
                        Some(display_name),
                        &text,
                        self as &dyn StreamResponder,
                        &channel,
                        opengoose_core::ThrottlePolicy::slack(),
                        SLACK_MAX_LEN,
                    )
                    .await
                {
                    // Error event is emitted by bridge; just log here.
                    error!(%error, "failed to relay slack message");
                }
            }
            SlackEnvelopeAction::TeamCommand(ref cmd) => {
                debug!(command = ?cmd.command, "handling slack team command");
                self.handle_team_command(cmd).await;
            }
        }
    }
}

fn team_command_session_key(cmd: &SlashCommand) -> Option<SessionKey> {
    let channel_id = cmd.channel_id.as_deref()?;
    let team_id = cmd.team_id.as_deref().unwrap_or("unknown");
    Some(SessionKey::new(Platform::Slack, team_id, channel_id))
}

fn team_command_args(cmd: &SlashCommand) -> &str {
    cmd.text.as_deref().unwrap_or("").trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slash_command() -> SlashCommand {
        SlashCommand {
            command: Some("/team".to_string()),
            text: Some(" ops ".to_string()),
            channel_id: Some("C123".to_string()),
            team_id: Some("T123".to_string()),
            user_name: Some("alice".to_string()),
            response_url: Some("https://hooks.slack.com/commands/123".to_string()),
        }
    }

    #[test]
    fn test_team_command_session_key_preserves_slack_team_and_channel() {
        let session_key = team_command_session_key(&slash_command()).expect("session key");
        assert_eq!(session_key.platform, Platform::Slack);
        assert_eq!(session_key.namespace.as_deref(), Some("T123"));
        assert_eq!(session_key.channel_id, "C123");
    }

    #[test]
    fn test_team_command_session_key_defaults_team_to_unknown() {
        let mut cmd = slash_command();
        cmd.team_id = None;

        let session_key = team_command_session_key(&cmd).expect("session key");
        assert_eq!(session_key.namespace.as_deref(), Some("unknown"));
    }

    #[test]
    fn test_team_command_session_key_requires_channel_id() {
        let mut cmd = slash_command();
        cmd.channel_id = None;

        assert!(team_command_session_key(&cmd).is_none());
    }

    #[test]
    fn test_team_command_args_trim_whitespace() {
        assert_eq!(team_command_args(&slash_command()), "ops");
    }

    #[test]
    fn test_team_command_args_default_to_empty_string() {
        let mut cmd = slash_command();
        cmd.text = None;

        assert_eq!(team_command_args(&cmd), "");
    }
}
