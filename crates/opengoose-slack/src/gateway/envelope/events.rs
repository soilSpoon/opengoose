use opengoose_types::{Platform, SessionKey};
use serde_json::Value;

use crate::types::{EventCallback, SlackEvent};

use super::super::types::SlackEnvelopeAction;

pub(super) fn classify_events_api(payload: &Value, bot_user_id: &str) -> SlackEnvelopeAction {
    match serde_json::from_value::<EventCallback>(payload.clone()) {
        Ok(callback) => relay_action(callback, bot_user_id).unwrap_or(SlackEnvelopeAction::Ignore),
        Err(_) => SlackEnvelopeAction::Ignore,
    }
}

fn relay_action(callback: EventCallback, bot_user_id: &str) -> Option<SlackEnvelopeAction> {
    let EventCallback { team_id, event } = callback;
    let relay = RelayMessage::from_event(event?, bot_user_id)?;
    let team_id = team_id.unwrap_or_else(|| "unknown".to_string());

    Some(SlackEnvelopeAction::Relay {
        session_key: SessionKey::new(Platform::Slack, team_id, &relay.channel),
        channel: relay.channel,
        text: relay.text,
        display_name: relay.display_name,
    })
}

struct RelayMessage {
    channel: String,
    text: String,
    display_name: String,
}

impl RelayMessage {
    fn from_event(event: SlackEvent, bot_user_id: &str) -> Option<Self> {
        let SlackEvent {
            event_type,
            channel,
            user,
            text,
            bot_id,
            subtype,
        } = event;

        if event_type != "message" || subtype.is_some() || bot_id.is_some() {
            return None;
        }

        if user.as_deref() == Some(bot_user_id) {
            return None;
        }

        Some(Self {
            channel: channel?,
            text: trimmed_text(text?)?,
            display_name: user?,
        })
    }
}

fn trimmed_text(text: String) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}
