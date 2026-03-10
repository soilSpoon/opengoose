//! Internal gateway action types.

use opengoose_types::SessionKey;

use crate::types::SlashCommand;

/// Action derived from classifying a Socket Mode envelope.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SlackEnvelopeAction {
    Ignore,
    Relay {
        session_key: SessionKey,
        channel: String,
        text: String,
        display_name: String,
    },
    TeamCommand(SlashCommand),
}
