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
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SlashCommand {
    pub command: Option<String>,
    pub text: Option<String>,
    pub channel_id: Option<String>,
    pub team_id: Option<String>,
    pub user_name: Option<String>,
    pub response_url: Option<String>,
}

/// Response from `auth.test`.
#[derive(Deserialize)]
pub struct AuthTestResponse {
    pub ok: bool,
    pub user_id: Option<String>,
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

    #[test]
    fn test_deserialize_connections_open_error() {
        let json = r#"{"ok":false,"error":"invalid_auth"}"#;
        let resp: ConnectionsOpenResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.error.unwrap(), "invalid_auth");
        assert!(resp.url.is_none());
    }

    #[test]
    fn test_deserialize_event_callback_no_event() {
        let json = r#"{"team_id":"T123"}"#;
        let cb: EventCallback = serde_json::from_str(json).unwrap();
        assert_eq!(cb.team_id.unwrap(), "T123");
        assert!(cb.event.is_none());
    }

    #[test]
    fn test_deserialize_slack_event_with_subtype() {
        let json = r#"{"type":"message","channel":"C123","subtype":"channel_join"}"#;
        let event: SlackEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "message");
        assert_eq!(event.subtype.unwrap(), "channel_join");
        assert!(event.user.is_none());
        assert!(event.text.is_none());
    }

    #[test]
    fn test_deserialize_slack_event_with_bot_id() {
        let json = r#"{"type":"message","channel":"C123","bot_id":"B123","text":"bot msg"}"#;
        let event: SlackEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.bot_id.unwrap(), "B123");
    }

    #[test]
    fn test_serialize_envelope_ack_with_payload() {
        let ack = EnvelopeAck {
            envelope_id: "abc".to_string(),
            payload: Some(serde_json::json!({"text": "response"})),
        };
        let json = serde_json::to_string(&ack).unwrap();
        assert!(json.contains("payload"));
        assert!(json.contains("response"));
    }

    #[test]
    fn test_deserialize_socket_envelope_no_payload() {
        let json = r#"{"envelope_id":"abc123","type":"hello"}"#;
        let env: SocketEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(env.envelope_id, "abc123");
        assert_eq!(env.envelope_type, "hello");
        assert!(env.payload.is_none());
    }

    #[test]
    fn test_deserialize_auth_test_response_success() {
        let json = r#"{"ok":true,"user_id":"U123","bot_id":"B456"}"#;
        let resp: AuthTestResponse = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.user_id.unwrap(), "U123");
    }

    #[test]
    fn test_deserialize_auth_test_response_error() {
        let json = r#"{"ok":false,"error":"invalid_auth"}"#;
        let resp: AuthTestResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.error.unwrap(), "invalid_auth");
    }

    #[test]
    fn test_deserialize_post_message_response_with_ts() {
        let json = r#"{"ok":true,"ts":"1234567890.123456"}"#;
        let resp: PostMessageResponse = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.ts.unwrap(), "1234567890.123456");
    }

    #[test]
    fn test_deserialize_post_message_response_error() {
        let json = r#"{"ok":false,"error":"channel_not_found"}"#;
        let resp: PostMessageResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.error.unwrap(), "channel_not_found");
    }

    #[test]
    fn test_deserialize_chat_update_response() {
        let json = r#"{"ok":true}"#;
        let resp: ChatUpdateResponse = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
    }

    #[test]
    fn test_deserialize_slash_command_minimal() {
        let json = r#"{}"#;
        let cmd: SlashCommand = serde_json::from_str(json).unwrap();
        assert!(cmd.command.is_none());
        assert!(cmd.text.is_none());
        assert!(cmd.channel_id.is_none());
        assert!(cmd.team_id.is_none());
        assert!(cmd.response_url.is_none());
    }
}
