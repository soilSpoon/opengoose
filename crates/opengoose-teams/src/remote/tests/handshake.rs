use crate::remote::protocol::{ConnectionState, ProtocolMessage};
use crate::remote::registry::RemoteAgentRegistry;

use super::test_config;

#[test]
fn protocol_message_serialization() {
    let msg = ProtocolMessage::Handshake {
        agent_name: "remote-1".into(),
        api_key: "key".into(),
        capabilities: vec!["code-review".into()],
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"handshake\""));
    assert!(json.contains("remote-1"));

    let parsed: ProtocolMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        ProtocolMessage::Handshake {
            agent_name,
            api_key,
            capabilities,
        } => {
            assert_eq!(agent_name, "remote-1");
            assert_eq!(api_key, "key");
            assert_eq!(capabilities, vec!["code-review"]);
        }
        _ => unreachable!("wrong variant"),
    }
}

#[test]
fn all_protocol_messages_roundtrip() {
    let messages = vec![
        ProtocolMessage::HandshakeAck {
            success: true,
            error: None,
        },
        ProtocolMessage::Heartbeat { timestamp: 12345 },
        ProtocolMessage::MessageRelay {
            from: "a".into(),
            to: "b".into(),
            payload: "hello".into(),
        },
        ProtocolMessage::Broadcast {
            from: "a".into(),
            channel: "news".into(),
            payload: "update".into(),
        },
        ProtocolMessage::Disconnect {
            reason: "shutdown".into(),
        },
        ProtocolMessage::Error {
            message: "oops".into(),
        },
    ];
    for msg in messages {
        let json = serde_json::to_string(&msg).unwrap();
        let _: ProtocolMessage = serde_json::from_str(&json).unwrap();
    }
}

#[test]
fn validate_key_accepts_valid() {
    let reg = RemoteAgentRegistry::new(test_config());
    assert!(reg.validate_key("test-key-123"));
    assert!(!reg.validate_key("wrong-key"));
}

#[test]
fn validate_key_open_when_no_keys() {
    let config = RemoteConfig {
        api_keys: vec![],
        ..Default::default()
    };
    let reg = RemoteAgentRegistry::new(config);
    assert!(reg.validate_key("anything"));
}

use crate::remote::registry::RemoteConfig;

#[test]
fn handshake_ack_error_roundtrip() {
    let msg = ProtocolMessage::HandshakeAck {
        success: false,
        error: Some("invalid api key".into()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"handshake_ack\""));
    assert!(json.contains("invalid api key"));
    let parsed: ProtocolMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        ProtocolMessage::HandshakeAck {
            success,
            error: Some(e),
        } => {
            assert!(!success);
            assert_eq!(e, "invalid api key");
        }
        _ => unreachable!("wrong variant"),
    }
}

#[test]
fn heartbeat_default_timestamp_is_nonzero() {
    let json = r#"{"type":"heartbeat"}"#;
    let msg: ProtocolMessage = serde_json::from_str(json).unwrap();
    match msg {
        ProtocolMessage::Heartbeat { timestamp } => {
            assert!(timestamp > 0);
        }
        _ => unreachable!("wrong variant"),
    }
}

#[test]
fn reconnect_and_reconnect_ack_roundtrip() {
    let reconnect = ProtocolMessage::Reconnect { last_event_id: 42 };
    let json = serde_json::to_string(&reconnect).unwrap();
    assert!(json.contains("\"type\":\"reconnect\""));
    assert!(json.contains("42"));

    let parsed: ProtocolMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        ProtocolMessage::Reconnect { last_event_id } => assert_eq!(last_event_id, 42),
        _ => unreachable!("wrong variant"),
    }

    let ack = ProtocolMessage::ReconnectAck {
        success: true,
        replayed_events: 0,
    };
    let json = serde_json::to_string(&ack).unwrap();
    assert!(json.contains("\"type\":\"reconnect_ack\""));
    let parsed: ProtocolMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        ProtocolMessage::ReconnectAck {
            success,
            replayed_events,
        } => {
            assert!(success);
            assert_eq!(replayed_events, 0);
        }
        _ => unreachable!("wrong variant"),
    }
}

#[test]
fn connection_state_serialization() {
    for (state, expected) in [
        (ConnectionState::Connecting, "connecting"),
        (ConnectionState::Connected, "connected"),
        (ConnectionState::Disconnecting, "disconnecting"),
        (ConnectionState::Reconnecting, "reconnecting"),
    ] {
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, format!("\"{}\"", expected));
    }
}

#[test]
fn reconnect_with_zero_last_event_id_roundtrip() {
    let json = r#"{"type":"reconnect"}"#;
    let msg: ProtocolMessage = serde_json::from_str(json).unwrap();
    match msg {
        ProtocolMessage::Reconnect { last_event_id } => assert_eq!(last_event_id, 0),
        _ => unreachable!("wrong variant"),
    }
}
