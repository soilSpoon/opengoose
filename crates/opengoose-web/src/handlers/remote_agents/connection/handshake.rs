use std::time::Duration;

use axum::extract::ws::WebSocket;
use opengoose_teams::remote::ProtocolMessage;
use tracing::warn;

use super::super::RemoteGatewayState;
use super::socket_io::{recv_text, send_protocol};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct AcceptedHandshake {
    pub(super) name: String,
    pub(super) capabilities: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum HandshakeError {
    InvalidApiKey,
    ExpectedHandshakeMessage,
}

pub(super) async fn negotiate_handshake(
    socket: &mut WebSocket,
    state: &RemoteGatewayState,
    endpoint: &str,
) -> Option<AcceptedHandshake> {
    let handshake_msg = match tokio::time::timeout(Duration::from_secs(10), recv_text(socket)).await
    {
        Ok(Some(text)) => text,
        Ok(None) => {
            warn!(%endpoint, "connection closed before handshake");
            return None;
        }
        Err(_) => {
            warn!(%endpoint, "handshake timeout");
            let _ = send_protocol(
                socket,
                &ProtocolMessage::Error {
                    message: "handshake timeout".into(),
                },
            )
            .await;
            return None;
        }
    };

    parse_handshake(state, socket, &handshake_msg).await
}

async fn parse_handshake(
    state: &RemoteGatewayState,
    socket: &mut WebSocket,
    handshake_msg: &str,
) -> Option<AcceptedHandshake> {
    match decode_handshake(handshake_msg, |api_key| {
        state.registry.validate_key(api_key)
    }) {
        Ok(handshake) => Some(handshake),
        Err(HandshakeError::InvalidApiKey) => {
            let _ = send_protocol(
                socket,
                &ProtocolMessage::HandshakeAck {
                    success: false,
                    error: Some("invalid api key".into()),
                },
            )
            .await;
            None
        }
        Err(HandshakeError::ExpectedHandshakeMessage) => {
            let _ = send_protocol(
                socket,
                &ProtocolMessage::Error {
                    message: "expected handshake message".into(),
                },
            )
            .await;
            None
        }
    }
}

fn decode_handshake(
    handshake_msg: &str,
    validate_key: impl Fn(&str) -> bool,
) -> Result<AcceptedHandshake, HandshakeError> {
    match serde_json::from_str::<ProtocolMessage>(handshake_msg) {
        Ok(ProtocolMessage::Handshake {
            agent_name: name,
            api_key,
            capabilities,
        }) if validate_key(&api_key) => Ok(AcceptedHandshake { name, capabilities }),
        Ok(ProtocolMessage::Handshake { .. }) => Err(HandshakeError::InvalidApiKey),
        _ => Err(HandshakeError::ExpectedHandshakeMessage),
    }
}

#[cfg(test)]
mod tests {
    use opengoose_teams::remote::ProtocolMessage;

    use super::{AcceptedHandshake, HandshakeError, decode_handshake};

    #[test]
    fn decode_handshake_accepts_valid_handshake() {
        let handshake_msg = serde_json::to_string(&ProtocolMessage::Handshake {
            agent_name: "remote-a".into(),
            api_key: "valid-key".into(),
            capabilities: vec!["relay".into()],
        })
        .expect("handshake should serialize");

        let handshake = decode_handshake(&handshake_msg, |key| key == "valid-key")
            .expect("handshake should be accepted");

        assert_eq!(
            handshake,
            AcceptedHandshake {
                name: "remote-a".into(),
                capabilities: vec!["relay".into()],
            }
        );
    }

    #[test]
    fn decode_handshake_rejects_invalid_api_key() {
        let handshake_msg = serde_json::to_string(&ProtocolMessage::Handshake {
            agent_name: "remote-a".into(),
            api_key: "wrong-key".into(),
            capabilities: vec![],
        })
        .expect("handshake should serialize");

        assert_eq!(
            decode_handshake(&handshake_msg, |key| key == "valid-key"),
            Err(HandshakeError::InvalidApiKey)
        );
    }

    #[test]
    fn decode_handshake_rejects_non_handshake_messages() {
        let handshake_msg = serde_json::to_string(&ProtocolMessage::Heartbeat { timestamp: 0 })
            .expect("heartbeat should serialize");

        assert_eq!(
            decode_handshake(&handshake_msg, |_| true),
            Err(HandshakeError::ExpectedHandshakeMessage)
        );
    }
}
