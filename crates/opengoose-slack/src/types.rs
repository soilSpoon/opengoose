//! Slack API types for Socket Mode and Web API interactions.

use serde::{Deserialize, Serialize};

/// Response from `apps.connections.open`.
#[derive(Deserialize)]
pub struct ConnectionsOpenResponse {
    pub ok: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

/// Socket Mode envelope wrapping all events.
#[derive(Deserialize)]
pub struct SocketEnvelope {
    pub envelope_id: String,
    #[serde(rename = "type")]
    pub envelope_type: String,
    pub payload: Option<serde_json::Value>,
}

/// ACK response sent back for each envelope.
#[derive(Serialize)]
pub struct EnvelopeAck {
    pub envelope_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// Event callback payload from events_api envelope.
#[derive(Deserialize)]
pub struct EventCallback {
    pub team_id: Option<String>,
    pub event: Option<SlackEvent>,
}

/// Individual Slack event.
#[derive(Deserialize)]
pub struct SlackEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub channel: Option<String>,
    pub user: Option<String>,
    pub text: Option<String>,
    pub bot_id: Option<String>,
    pub subtype: Option<String>,
}

/// Slash command payload.
///
/// Fields like `team_id` and `user_name` are populated by Slack's API
/// during deserialization even if not directly accessed in all code paths.
#[derive(Deserialize)]
pub struct SlashCommand {
    pub command: Option<String>,
    pub text: Option<String>,
    pub channel_id: Option<String>,
    pub team_id: Option<String>,
    #[allow(dead_code)] // deserialized from Slack API
    pub user_name: Option<String>,
    pub response_url: Option<String>,
}

/// Response from `auth.test`.
///
/// `bot_id` is deserialized from Slack's API response for completeness.
#[derive(Deserialize)]
pub struct AuthTestResponse {
    pub ok: bool,
    pub user_id: Option<String>,
    #[allow(dead_code)] // deserialized from Slack API
    pub bot_id: Option<String>,
    pub error: Option<String>,
}

/// Response from `chat.postMessage`.
#[derive(Deserialize)]
pub struct PostMessageResponse {
    pub ok: bool,
    pub ts: Option<String>,
    pub error: Option<String>,
}

/// Response from `chat.update`.
#[derive(Deserialize)]
pub struct ChatUpdateResponse {
    pub ok: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_connections_open() {
        let json = r#"{"ok":true,"url":"wss://example.com/ws"}"#;
        let resp: ConnectionsOpenResponse = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.url.unwrap(), "wss://example.com/ws");
    }

    #[test]
    fn test_deserialize_socket_envelope() {
        let json = r#"{"envelope_id":"abc123","type":"events_api","payload":{"team_id":"T123","event":{"type":"message","channel":"C123","user":"U456","text":"hello"}}}"#;
        let env: SocketEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(env.envelope_id, "abc123");
        assert_eq!(env.envelope_type, "events_api");
        assert!(env.payload.is_some());
    }

    #[test]
    fn test_serialize_envelope_ack() {
        let ack = EnvelopeAck {
            envelope_id: "abc".to_string(),
            payload: None,
        };
        let json = serde_json::to_string(&ack).unwrap();
        assert_eq!(json, r#"{"envelope_id":"abc"}"#);
    }

    #[test]
    fn test_deserialize_event_callback() {
        let json = r#"{"team_id":"T123","event":{"type":"message","channel":"C123","user":"U456","text":"hello"}}"#;
        let cb: EventCallback = serde_json::from_str(json).unwrap();
        assert_eq!(cb.team_id.unwrap(), "T123");
        let event = cb.event.unwrap();
        assert_eq!(event.event_type, "message");
        assert_eq!(event.text.unwrap(), "hello");
    }

    #[test]
    fn test_deserialize_slash_command() {
        let json = r#"{"command":"/team","text":"devops","channel_id":"C123","team_id":"T123","user_name":"alice","response_url":"https://hooks.slack.com/commands/xxx"}"#;
        let cmd: SlashCommand = serde_json::from_str(json).unwrap();
        assert_eq!(cmd.command.unwrap(), "/team");
        assert_eq!(cmd.text.unwrap(), "devops");
    }
}
