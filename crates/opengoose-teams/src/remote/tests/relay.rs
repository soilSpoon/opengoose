use crate::remote::protocol::ProtocolMessage;
use crate::remote::registry::{RemoteAgentRegistry, RemoteConfig};
use crate::remote::transport::ReplayResult;

use super::test_config;

#[tokio::test]
async fn send_to_connected_agent() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register("agent-z".into(), vec![], "ws://z".into(), tx)
        .await
        .unwrap();

    let msg = ProtocolMessage::MessageRelay {
        from: "local".into(),
        to: "agent-z".into(),
        payload: "test".into(),
    };
    assert!(reg.send_to("agent-z", msg).await);
    let received = rx.recv().await.unwrap();
    match received {
        ProtocolMessage::MessageRelay { payload, .. } => {
            assert_eq!(payload, "test");
        }
        _ => unreachable!("wrong message type"),
    }
}

#[tokio::test]
async fn send_to_disconnected_returns_false() {
    let reg = RemoteAgentRegistry::new(test_config());
    let msg = ProtocolMessage::Heartbeat { timestamp: 0 };
    assert!(!reg.send_to("ghost", msg).await);
}

#[tokio::test]
async fn send_to_dropped_receiver_returns_false() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register("dropped".into(), vec![], "ws://d".into(), tx)
        .await
        .unwrap();

    drop(rx);

    let msg = ProtocolMessage::Heartbeat { timestamp: 1 };
    assert!(!reg.send_to("dropped", msg).await);
}

#[tokio::test]
async fn replay_since_reenqueues_buffered_message_relays() {
    let reg = RemoteAgentRegistry::new(RemoteConfig {
        replay_buffer_capacity: 4,
        ..test_config()
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register("replay-agent".into(), vec![], "ws://replay".into(), tx)
        .await
        .unwrap();

    assert!(
        reg.send_to(
            "replay-agent",
            ProtocolMessage::MessageRelay {
                from: "local".into(),
                to: "replay-agent".into(),
                payload: "first".into(),
            },
        )
        .await
    );
    assert!(
        reg.send_to(
            "replay-agent",
            ProtocolMessage::MessageRelay {
                from: "local".into(),
                to: "replay-agent".into(),
                payload: "second".into(),
            },
        )
        .await
    );

    let _ = rx.recv().await.expect("first delivery should exist");
    let _ = rx.recv().await.expect("second delivery should exist");

    assert_eq!(
        reg.replay_since("replay-agent", 1).await,
        ReplayResult::Replayed(1)
    );

    match rx.recv().await.expect("replayed delivery should exist") {
        ProtocolMessage::MessageRelay { payload, .. } => assert_eq!(payload, "second"),
        other => panic!("expected replayed relay, got {other:?}"),
    }
}

#[tokio::test]
async fn replay_since_reenqueues_buffered_disconnects() {
    let reg = RemoteAgentRegistry::new(RemoteConfig {
        replay_buffer_capacity: 4,
        ..test_config()
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register(
        "disconnect-agent".into(),
        vec![],
        "ws://disconnect".into(),
        tx,
    )
    .await
    .unwrap();

    assert!(
        reg.send_to(
            "disconnect-agent",
            ProtocolMessage::Disconnect {
                reason: "server shutdown".into(),
            },
        )
        .await
    );

    let _ = rx.recv().await.expect("initial disconnect should exist");

    assert_eq!(
        reg.replay_since("disconnect-agent", 0).await,
        ReplayResult::Replayed(1)
    );

    match rx.recv().await.expect("replayed disconnect should exist") {
        ProtocolMessage::Disconnect { reason } => assert_eq!(reason, "server shutdown"),
        other => panic!("expected replayed disconnect, got {other:?}"),
    }
}

#[tokio::test]
async fn replay_since_reenqueues_buffered_broadcasts() {
    let reg = RemoteAgentRegistry::new(RemoteConfig {
        replay_buffer_capacity: 4,
        ..test_config()
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register(
        "broadcast-agent".into(),
        vec![],
        "ws://broadcast".into(),
        tx,
    )
    .await
    .unwrap();

    assert!(
        reg.send_to(
            "broadcast-agent",
            ProtocolMessage::Broadcast {
                from: "ops".into(),
                channel: "alerts".into(),
                payload: "rotate credentials".into(),
            },
        )
        .await
    );

    let _ = rx.recv().await.expect("initial broadcast should exist");

    assert_eq!(
        reg.replay_since("broadcast-agent", 0).await,
        ReplayResult::Replayed(1)
    );

    match rx.recv().await.expect("replayed broadcast should exist") {
        ProtocolMessage::Broadcast {
            from,
            channel,
            payload,
        } => {
            assert_eq!(from, "ops");
            assert_eq!(channel, "alerts");
            assert_eq!(payload, "rotate credentials");
        }
        other => panic!("expected replayed broadcast, got {other:?}"),
    }
}

#[tokio::test]
async fn replay_since_returns_buffer_miss_for_evicted_history() {
    let reg = RemoteAgentRegistry::new(RemoteConfig {
        replay_buffer_capacity: 2,
        ..test_config()
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register("window-agent".into(), vec![], "ws://window".into(), tx)
        .await
        .unwrap();

    for payload in ["one", "two", "three"] {
        assert!(
            reg.send_to(
                "window-agent",
                ProtocolMessage::MessageRelay {
                    from: "local".into(),
                    to: "window-agent".into(),
                    payload: payload.into(),
                },
            )
            .await
        );
    }

    for _ in 0..3 {
        let _ = rx.recv().await.expect("initial delivery should exist");
    }

    assert_eq!(
        reg.replay_since("window-agent", 0).await,
        ReplayResult::BufferMiss
    );
}

#[tokio::test]
async fn replay_since_returns_unavailable_for_unknown_agent() {
    let reg = RemoteAgentRegistry::new(test_config());
    assert_eq!(
        reg.replay_since("ghost", 0).await,
        ReplayResult::Unavailable
    );
}
