use serde::{Deserialize, Serialize};

/// Protocol message types exchanged over the WebSocket connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolMessage {
    /// Client → Server: initial authentication.
    Handshake {
        agent_name: String,
        api_key: String,
        #[serde(default)]
        capabilities: Vec<String>,
    },
    /// Server → Client: handshake result.
    HandshakeAck {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Bidirectional: keep-alive ping.
    Heartbeat {
        #[serde(default = "default_timestamp")]
        timestamp: u64,
    },
    /// Server → Client or Client → Server: relay a message.
    MessageRelay {
        from: String,
        to: String,
        payload: String,
    },
    /// Server → Client: broadcast from a channel.
    Broadcast {
        from: String,
        channel: String,
        payload: String,
    },
    /// Client → Server: agent wants to disconnect gracefully.
    Disconnect { reason: String },
    /// Server → Client: error notification.
    Error { message: String },
    /// Client → Server: reconnect after a drop, providing the last seen event ID.
    Reconnect {
        #[serde(default)]
        last_event_id: u64,
    },
    /// Server → Client: reconnect acknowledgement.
    ReconnectAck {
        success: bool,
        /// Number of events replayed since last_event_id (0 if no replay buffer).
        replayed_events: u64,
    },
}

fn default_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::ProtocolMessage;

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
    fn reconnect_with_zero_last_event_id_roundtrip() {
        let json = r#"{"type":"reconnect"}"#;
        let msg: ProtocolMessage = serde_json::from_str(json).unwrap();
        match msg {
            ProtocolMessage::Reconnect { last_event_id } => assert_eq!(last_event_id, 0),
            _ => unreachable!("wrong variant"),
        }
    }
}
