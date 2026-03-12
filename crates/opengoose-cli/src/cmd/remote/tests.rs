use futures_util::{SinkExt, StreamExt};
use opengoose_teams::remote::ProtocolMessage;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use super::connect::{
    ConnectFailure, ConnectMode, build_connect_url, connect_session, reconnect_delay,
};
use super::http::{RemoteAgentInfo, disconnect_url, format_duration, list_url};
use super::protocol::{next_delivery_label, record_replayable_event};

#[test]
fn format_duration_seconds() {
    assert_eq!(format_duration(0), "0s ago");
    assert_eq!(format_duration(1), "1s ago");
    assert_eq!(format_duration(59), "59s ago");
}

#[test]
fn format_duration_minutes() {
    assert_eq!(format_duration(60), "1m ago");
    assert_eq!(format_duration(90), "1m ago");
    assert_eq!(format_duration(120), "2m ago");
    assert_eq!(format_duration(3599), "59m ago");
}

#[test]
fn format_duration_hours() {
    assert_eq!(format_duration(3600), "1h ago");
    assert_eq!(format_duration(7200), "2h ago");
    assert_eq!(format_duration(36000), "10h ago");
}

#[test]
fn ws_url_appends_connect_path_when_not_present() {
    assert_eq!(
        build_connect_url("ws://localhost:8080"),
        "ws://localhost:8080/api/agents/connect"
    );
}

#[test]
fn ws_url_preserves_full_connect_path() {
    assert_eq!(
        build_connect_url("ws://localhost:8080/api/agents/connect"),
        "ws://localhost:8080/api/agents/connect"
    );
}

#[test]
fn ws_url_trims_trailing_slash_before_appending() {
    assert_eq!(
        build_connect_url("ws://localhost:8080/"),
        "ws://localhost:8080/api/agents/connect"
    );
}

#[test]
fn list_url_construction() {
    assert_eq!(
        list_url("http://127.0.0.1:8080"),
        "http://127.0.0.1:8080/api/agents/remote"
    );
}

#[test]
fn list_url_construction_trims_trailing_slash() {
    assert_eq!(
        list_url("http://127.0.0.1:8080/"),
        "http://127.0.0.1:8080/api/agents/remote"
    );
}

#[test]
fn disconnect_url_construction_encodes_name() {
    assert_eq!(
        disconnect_url("http://127.0.0.1:8080", "my agent"),
        "http://127.0.0.1:8080/api/agents/remote/my%20agent"
    );
}

#[test]
fn remote_agent_info_deserializes_correctly() {
    let json = r#"{
        "name": "test-agent",
        "capabilities": ["chat", "code"],
        "endpoint": "ws://localhost:9000",
        "connected_secs": 120,
        "last_heartbeat_secs": 5
    }"#;
    let info: RemoteAgentInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.name, "test-agent");
    assert_eq!(info.capabilities, vec!["chat", "code"]);
    assert_eq!(info.endpoint, "ws://localhost:9000");
    assert_eq!(info.connected_secs, 120);
    assert_eq!(info.last_heartbeat_secs, 5);
}

#[test]
fn remote_agent_info_formats_duration_for_display() {
    let info = RemoteAgentInfo {
        name: "agent".into(),
        capabilities: vec![],
        endpoint: "ws://localhost:9000".into(),
        connected_secs: 3660,
        last_heartbeat_secs: 30,
    };
    assert_eq!(format_duration(info.connected_secs), "1h ago");
    assert_eq!(format_duration(info.last_heartbeat_secs), "30s ago");
}

#[test]
fn wss_url_appends_connect_path() {
    assert_eq!(
        build_connect_url("wss://example.com:8443"),
        "wss://example.com:8443/api/agents/connect"
    );
}

#[test]
fn wss_url_preserves_full_connect_path() {
    assert_eq!(
        build_connect_url("wss://example.com:8443/api/agents/connect"),
        "wss://example.com:8443/api/agents/connect"
    );
}

#[test]
fn wss_url_trims_trailing_slash_before_appending() {
    assert_eq!(
        build_connect_url("wss://example.com:8443/"),
        "wss://example.com:8443/api/agents/connect"
    );
}

#[test]
fn reconnect_delay_caps_after_backoff_growth() {
    assert_eq!(reconnect_delay(0), std::time::Duration::from_secs(1));
    assert_eq!(reconnect_delay(1), std::time::Duration::from_secs(2));
    assert_eq!(reconnect_delay(2), std::time::Duration::from_secs(4));
    assert_eq!(reconnect_delay(3), std::time::Duration::from_secs(5));
    assert_eq!(reconnect_delay(8), std::time::Duration::from_secs(5));
}

#[test]
fn record_replayable_event_only_counts_replayable_messages() {
    let mut last_seen_event_id = 0;

    assert_eq!(
        record_replayable_event(
            &mut last_seen_event_id,
            &ProtocolMessage::MessageRelay {
                from: "a".into(),
                to: "b".into(),
                payload: "hello".into(),
            },
        ),
        Some(1)
    );
    assert_eq!(
        record_replayable_event(
            &mut last_seen_event_id,
            &ProtocolMessage::Heartbeat { timestamp: 0 },
        ),
        None
    );
    assert_eq!(
        record_replayable_event(
            &mut last_seen_event_id,
            &ProtocolMessage::Broadcast {
                from: "ops".into(),
                channel: "alerts".into(),
                payload: "hi".into(),
            },
        ),
        Some(2)
    );
}

#[test]
fn next_delivery_label_consumes_pending_replays() {
    let mut pending = 2;
    assert_eq!(next_delivery_label(&mut pending), "replay");
    assert_eq!(next_delivery_label(&mut pending), "replay");
    assert_eq!(next_delivery_label(&mut pending), "live");
    assert_eq!(pending, 0);
}

async fn recv_protocol_message(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> ProtocolMessage {
    let message = socket
        .next()
        .await
        .expect("socket should yield a message")
        .expect("socket message should be valid");
    let text = message.into_text().expect("message should be text");
    serde_json::from_str(&text).expect("message should deserialize")
}

#[tokio::test]
async fn connect_session_sends_reconnect_request_with_last_seen_event_id() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();

        match recv_protocol_message(&mut socket).await {
            ProtocolMessage::Handshake {
                agent_name,
                api_key,
                capabilities,
            } => {
                assert_eq!(agent_name, "resume-agent");
                assert_eq!(api_key, "secret");
                assert!(capabilities.is_empty());
            }
            other => panic!("expected handshake, got {other:?}"),
        }

        socket
            .send(Message::Text(
                serde_json::to_string(&ProtocolMessage::HandshakeAck {
                    success: true,
                    error: None,
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();

        match recv_protocol_message(&mut socket).await {
            ProtocolMessage::Reconnect { last_event_id } => {
                assert_eq!(last_event_id, 7);
            }
            other => panic!("expected reconnect, got {other:?}"),
        }

        socket
            .send(Message::Text(
                serde_json::to_string(&ProtocolMessage::ReconnectAck {
                    success: true,
                    replayed_events: 2,
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
    });

    let session = connect_session(
        &build_connect_url(&format!("ws://{}", addr)),
        Some("secret"),
        "resume-agent",
        ConnectMode::Resume { last_event_id: 7 },
    )
    .await
    .expect("resume handshake should succeed");

    assert_eq!(session.replayed_events, 2);
    drop(session);
    server.await.unwrap();
}

#[tokio::test]
async fn connect_session_returns_terminal_error_when_resume_is_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();

        let _ = recv_protocol_message(&mut socket).await;
        socket
            .send(Message::Text(
                serde_json::to_string(&ProtocolMessage::HandshakeAck {
                    success: true,
                    error: None,
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();

        let _ = recv_protocol_message(&mut socket).await;
        socket
            .send(Message::Text(
                serde_json::to_string(&ProtocolMessage::ReconnectAck {
                    success: false,
                    replayed_events: 0,
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
    });

    let err = match connect_session(
        &build_connect_url(&format!("ws://{}", addr)),
        None,
        "resume-agent",
        ConnectMode::Resume { last_event_id: 3 },
    )
    .await
    {
        Ok(_) => panic!("resume rejection should be terminal"),
        Err(err) => err,
    };

    match err {
        ConnectFailure::Terminal(err) => {
            assert!(err.to_string().contains("resume rejected"));
        }
        ConnectFailure::Retryable(err) => {
            panic!("expected terminal error, got retryable: {err}");
        }
    }

    server.await.unwrap();
}
