use super::*;
use crate::types::SocketEnvelope;

#[test]
fn test_slack_envelope_relay_filter_ignores_bot_message() {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-1".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "message",
                "channel": "C1",
                "user": "U1",
                "text": "hello",
                "bot_id": "B1",
            }
        })),
    };
    assert!(matches!(
        classify_slack_envelope(&envelope, "U1"),
        SlackEnvelopeAction::Ignore
    ));
}

#[test]
fn test_slack_envelope_relay_filter_ignores_subtype() {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-2".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "message",
                "channel": "C1",
                "user": "U1",
                "text": "hello",
                "subtype": "channel_join"
            }
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_slack_envelope_relay_message() {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-3".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "message",
                "channel": "C1",
                "user": "U2",
                "text": "   hello   ",
            }
        })),
    };
    let action = classify_slack_envelope(&envelope, "BOT");
    assert!(matches!(
        action,
        SlackEnvelopeAction::Relay {
            session_key,
            channel,
            ref text,
            display_name
        } if session_key.platform == Platform::Slack
            && session_key.namespace == Some("T123".to_string())
            && session_key.channel_id == "C1"
            && channel == "C1"
            && text == "hello"
            && display_name == "U2"
    ));
}

#[test]
fn test_slack_envelope_team_command() -> Result<(), String> {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-4".to_string(),
        envelope_type: "slash_commands".to_string(),
        payload: Some(serde_json::json!({
            "command": "/team",
            "text": "ops",
            "channel_id": "C1",
            "team_id": "T123",
        })),
    };

    let action = classify_slack_envelope(&envelope, "BOT");
    let SlackEnvelopeAction::TeamCommand(cmd) = action else {
        return Err("expected team command".to_string());
    };
    assert_eq!(cmd.command.as_deref(), Some("/team"));
    assert_eq!(cmd.text.as_deref(), Some("ops"));
    Ok(())
}

#[test]
fn test_slack_envelope_ignores_self_message_by_user_id() {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-5".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "message",
                "channel": "C1",
                "user": "BOT_USER_ID",
                "text": "I said this myself",
            }
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT_USER_ID"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_slack_envelope_ignores_empty_text() {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-6".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "message",
                "channel": "C1",
                "user": "U1",
                "text": "   ",
            }
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_slack_envelope_ignores_unknown_type() {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-7".to_string(),
        envelope_type: "hello".to_string(),
        payload: Some(serde_json::json!({"type": "hello"})),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_slack_envelope_ignores_non_team_slash_command() {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-8".to_string(),
        envelope_type: "slash_commands".to_string(),
        payload: Some(serde_json::json!({
            "command": "/other",
            "text": "something",
            "channel_id": "C1",
            "team_id": "T123",
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_slack_envelope_ignores_no_payload() {
    let envelope = SocketEnvelope {
        envelope_id: "envelope-9".to_string(),
        envelope_type: "events_api".to_string(),
        payload: None,
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_classify_envelope_events_api_no_channel() {
    let envelope = SocketEnvelope {
        envelope_id: "e1".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "message",
                "user": "U1",
                "text": "hello",
            }
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_classify_envelope_events_api_no_text() {
    let envelope = SocketEnvelope {
        envelope_id: "e2".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "message",
                "channel": "C1",
                "user": "U1",
            }
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_classify_envelope_events_api_non_message_type() {
    let envelope = SocketEnvelope {
        envelope_id: "e3".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "reaction_added",
                "channel": "C1",
                "user": "U1",
                "text": "hello",
            }
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_classify_envelope_events_api_no_user() {
    let envelope = SocketEnvelope {
        envelope_id: "e4".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
            "event": {
                "type": "message",
                "channel": "C1",
                "text": "hello",
            }
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}

#[test]
fn test_classify_envelope_missing_team_id_defaults_to_unknown() -> Result<(), String> {
    let envelope = SocketEnvelope {
        envelope_id: "e5".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "event": {
                "type": "message",
                "channel": "C99",
                "user": "U7",
                "text": "no team",
            }
        })),
    };
    let action = classify_slack_envelope(&envelope, "BOT");
    let SlackEnvelopeAction::Relay { session_key, .. } = action else {
        return Err("expected Relay action".to_string());
    };
    assert_eq!(session_key.namespace.as_deref(), Some("unknown"));
    assert_eq!(session_key.channel_id, "C99");
    Ok(())
}

#[test]
fn test_classify_envelope_relay_session_key_uses_team_and_channel() -> Result<(), String> {
    let envelope = SocketEnvelope {
        envelope_id: "e6".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "TWORKSPACE",
            "event": {
                "type": "message",
                "channel": "CCHANNEL",
                "user": "UUSER",
                "text": "test message",
            }
        })),
    };
    let action = classify_slack_envelope(&envelope, "BOT");
    let SlackEnvelopeAction::Relay {
        session_key,
        channel,
        text,
        display_name,
    } = action
    else {
        return Err("expected Relay action".to_string());
    };
    assert_eq!(session_key.platform, Platform::Slack);
    assert_eq!(session_key.namespace.as_deref(), Some("TWORKSPACE"));
    assert_eq!(session_key.channel_id, "CCHANNEL");
    assert_eq!(channel, "CCHANNEL");
    assert_eq!(text, "test message");
    assert_eq!(display_name, "UUSER");
    Ok(())
}

#[test]
fn test_classify_envelope_text_whitespace_trimming() -> Result<(), String> {
    let envelope = SocketEnvelope {
        envelope_id: "e7".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T1",
            "event": {
                "type": "message",
                "channel": "C1",
                "user": "U1",
                "text": "\t  trimmed content  \n",
            }
        })),
    };
    let action = classify_slack_envelope(&envelope, "BOT");
    let SlackEnvelopeAction::Relay { text, .. } = action else {
        return Err("expected Relay action".to_string());
    };
    assert_eq!(text, "trimmed content");
    Ok(())
}

#[test]
fn test_classify_envelope_slash_command_no_response_url() -> Result<(), String> {
    let envelope = SocketEnvelope {
        envelope_id: "e8".to_string(),
        envelope_type: "slash_commands".to_string(),
        payload: Some(serde_json::json!({
            "command": "/team",
            "text": "status",
            "channel_id": "C1",
            "team_id": "T1",
        })),
    };
    let action = classify_slack_envelope(&envelope, "BOT");
    let SlackEnvelopeAction::TeamCommand(cmd) = action else {
        return Err("expected TeamCommand".to_string());
    };
    assert_eq!(cmd.command.as_deref(), Some("/team"));
    assert!(cmd.response_url.is_none());
    Ok(())
}

#[test]
fn test_classify_envelope_slash_command_preserves_user_name() -> Result<(), String> {
    let envelope = SocketEnvelope {
        envelope_id: "e9".to_string(),
        envelope_type: "slash_commands".to_string(),
        payload: Some(serde_json::json!({
            "command": "/team",
            "text": "list",
            "channel_id": "C2",
            "team_id": "T2",
            "user_name": "alice",
            "response_url": "https://hooks.slack.com/xxx",
        })),
    };
    let action = classify_slack_envelope(&envelope, "BOT");
    let SlackEnvelopeAction::TeamCommand(cmd) = action else {
        return Err("expected TeamCommand".to_string());
    };
    assert_eq!(cmd.user_name.as_deref(), Some("alice"));
    assert_eq!(cmd.text.as_deref(), Some("list"));
    assert_eq!(cmd.channel_id.as_deref(), Some("C2"));
    Ok(())
}

#[test]
fn test_classify_envelope_events_api_no_event_field() {
    let envelope = SocketEnvelope {
        envelope_id: "e10".to_string(),
        envelope_type: "events_api".to_string(),
        payload: Some(serde_json::json!({
            "team_id": "T123",
        })),
    };
    assert_eq!(
        classify_slack_envelope(&envelope, "BOT"),
        SlackEnvelopeAction::Ignore
    );
}
