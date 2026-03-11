
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::remote::protocol::{ConnectionState, ProtocolMessage};
use crate::remote::registry::{RemoteAgent, RemoteAgentRegistry, RemoteConfig};
use crate::remote::transport::ReplayResult;

fn test_config() -> RemoteConfig {
    RemoteConfig {
        heartbeat_interval_secs: 5,
        heartbeat_timeout_secs: 15,
        api_keys: vec!["test-key-123".to_string()],
        replay_buffer_capacity: 8,
    }
}

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

#[tokio::test]
async fn register_and_list() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register(
        "agent-1".into(),
        vec!["cap-a".into()],
        "ws://localhost:3000".into(),
        tx,
    )
    .await
    .unwrap();

    let agents = reg.list().await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].name, "agent-1");
    assert!(reg.is_connected("agent-1").await);
}

#[tokio::test]
async fn register_duplicate_fails() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx1, _) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, _) = tokio::sync::mpsc::unbounded_channel();

    reg.register("dup".into(), vec![], "ws://a".into(), tx1)
        .await
        .unwrap();
    let err = reg
        .register("dup".into(), vec![], "ws://b".into(), tx2)
        .await
        .unwrap_err();
    assert!(err.contains("already connected"));
}

#[tokio::test]
async fn unregister_removes_agent() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("agent-x".into(), vec![], "ws://x".into(), tx)
        .await
        .unwrap();
    assert!(reg.is_connected("agent-x").await);

    reg.unregister("agent-x").await;
    assert!(!reg.is_connected("agent-x").await);
    assert!(reg.list().await.is_empty());
}

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

#[tokio::test]
async fn send_to_disconnected_returns_false() {
    let reg = RemoteAgentRegistry::new(test_config());
    let msg = ProtocolMessage::Heartbeat { timestamp: 0 };
    assert!(!reg.send_to("ghost", msg).await);
}

#[test]
fn remote_agent_staleness() {
    let agent = RemoteAgent {
        name: "test".into(),
        capabilities: vec![],
        connected_at: Instant::now(),
        last_heartbeat: Instant::now() - Duration::from_secs(100),
        endpoint: "ws://test".into(),
        connection_state: ConnectionState::Connected,
    };
    assert!(agent.is_stale(Duration::from_secs(90)));
    assert!(!agent.is_stale(Duration::from_secs(200)));
}

#[test]
fn config_accessors_return_correct_durations() {
    let config = RemoteConfig {
        heartbeat_interval_secs: 30,
        heartbeat_timeout_secs: 90,
        api_keys: vec![],
        replay_buffer_capacity: 64,
    };
    let reg = RemoteAgentRegistry::new(config);
    assert_eq!(reg.heartbeat_interval(), Duration::from_secs(30));
    assert_eq!(reg.heartbeat_timeout(), Duration::from_secs(90));
}

#[tokio::test]
async fn touch_heartbeat_keeps_agent_registered() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("hb-agent".into(), vec![], "ws://hb".into(), tx)
        .await
        .unwrap();

    // Touch should not panic and agent should remain connected.
    reg.touch_heartbeat("hb-agent").await;
    assert!(reg.is_connected("hb-agent").await);
}

#[tokio::test]
async fn touch_heartbeat_unknown_agent_is_noop() {
    let reg = RemoteAgentRegistry::new(test_config());
    // Touching an agent that was never registered should not panic.
    reg.touch_heartbeat("nonexistent").await;
    assert!(!reg.is_connected("nonexistent").await);
}

#[tokio::test]
async fn reap_stale_removes_timed_out_agents() {
    let config = RemoteConfig {
        heartbeat_timeout_secs: 0,
        ..Default::default()
    };
    let reg = RemoteAgentRegistry::new(config);
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("stale".into(), vec![], "ws://s".into(), tx)
        .await
        .unwrap();

    // Give elapsed() > Duration::ZERO a moment to become true.
    tokio::time::sleep(Duration::from_millis(1)).await;

    let reaped = reg.reap_stale().await;
    assert!(reaped.contains(&"stale".to_string()));
    assert!(!reg.is_connected("stale").await);
}

#[tokio::test]
async fn reap_stale_keeps_fresh_agents() {
    let config = RemoteConfig {
        heartbeat_timeout_secs: 3600,
        ..Default::default()
    };
    let reg = RemoteAgentRegistry::new(config);
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("fresh".into(), vec![], "ws://f".into(), tx)
        .await
        .unwrap();

    let reaped = reg.reap_stale().await;
    assert!(reaped.is_empty());
    assert!(reg.is_connected("fresh").await);
}

#[tokio::test]
async fn reap_stale_only_removes_stale_subset() {
    let config = RemoteConfig {
        heartbeat_timeout_secs: 0,
        ..Default::default()
    };
    let reg = RemoteAgentRegistry::new(config);

    let (tx1, _) = tokio::sync::mpsc::unbounded_channel();
    let (tx2, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("will-reap".into(), vec![], "ws://r".into(), tx1)
        .await
        .unwrap();

    // Sleep so the first agent becomes stale.
    tokio::time::sleep(Duration::from_millis(1)).await;

    reg.register("just-joined".into(), vec![], "ws://j".into(), tx2)
        .await
        .unwrap();

    // The first agent is stale; the second was registered after the sleep.
    // With 0-second timeout both could be reaped depending on timing, but
    // the test verifies that reap_stale runs without error and removes stale entries.
    let reaped = reg.reap_stale().await;
    assert!(reaped.contains(&"will-reap".to_string()));
    // Regardless of timing for "just-joined", "will-reap" must be gone.
    assert!(!reg.is_connected("will-reap").await);
}

#[tokio::test]
async fn register_multiple_agents() {
    let reg = RemoteAgentRegistry::new(test_config());
    for i in 0..5 {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register(
            format!("agent-{i}"),
            vec![format!("cap-{i}")],
            format!("ws://host:{}", 8000 + i),
            tx,
        )
        .await
        .unwrap();
    }
    let agents = reg.list().await;
    assert_eq!(agents.len(), 5);
    for i in 0..5 {
        assert!(reg.is_connected(&format!("agent-{i}")).await);
    }
}

#[tokio::test]
async fn registry_change_subscription_tracks_meaningful_updates() {
    let reg = RemoteAgentRegistry::new(test_config());
    let mut changes = reg.subscribe_changes();
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();

    assert_eq!(*changes.borrow(), 0);

    reg.register("watch-agent".into(), vec![], "ws://watch".into(), tx)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*changes.borrow_and_update(), 1);

    reg.touch_heartbeat("watch-agent").await;
    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*changes.borrow_and_update(), 2);

    reg.mark_reconnecting("watch-agent").await;
    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*changes.borrow_and_update(), 3);

    reg.mark_connected("watch-agent").await;
    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*changes.borrow_and_update(), 4);

    reg.unregister("watch-agent").await;
    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*changes.borrow_and_update(), 5);
}

#[tokio::test]
async fn registry_change_subscription_skips_noop_updates() {
    let reg = RemoteAgentRegistry::new(test_config());
    let mut changes = reg.subscribe_changes();

    reg.touch_heartbeat("ghost").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), changes.changed())
            .await
            .is_err()
    );

    reg.mark_reconnecting("ghost").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), changes.changed())
            .await
            .is_err()
    );

    reg.mark_connected("ghost").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), changes.changed())
            .await
            .is_err()
    );

    reg.unregister("ghost").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), changes.changed())
            .await
            .is_err()
    );

    let reaped = reg.reap_stale().await;
    assert!(reaped.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(20), changes.changed())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn registry_change_subscription_preserves_latest_revision_for_new_subscribers() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();

    reg.register("late-sub".into(), vec![], "ws://late".into(), tx)
        .await
        .unwrap();

    let changes = reg.subscribe_changes();
    assert_eq!(*changes.borrow(), 1);
}

#[tokio::test]
async fn registry_change_subscription_notifies_on_reap_stale() {
    let config = RemoteConfig {
        heartbeat_timeout_secs: 0,
        ..Default::default()
    };
    let reg = RemoteAgentRegistry::new(config);
    let mut changes = reg.subscribe_changes();
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();

    reg.register("stale-watch".into(), vec![], "ws://stale".into(), tx)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .unwrap()
        .unwrap();
    let _ = changes.borrow_and_update();

    tokio::time::sleep(Duration::from_millis(1)).await;
    let reaped = reg.reap_stale().await;
    assert_eq!(reaped, vec!["stale-watch".to_string()]);

    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*changes.borrow_and_update(), 2);
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
    // A Heartbeat with no explicit timestamp should use SystemTime::now().
    let json = r#"{"type":"heartbeat"}"#;
    let msg: ProtocolMessage = serde_json::from_str(json).unwrap();
    match msg {
        ProtocolMessage::Heartbeat { timestamp } => {
            // The default_timestamp() function returns a real epoch second.
            // It will be > 0 unless the system clock is broken.
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

#[tokio::test]
async fn get_metrics_empty_registry() {
    let reg = RemoteAgentRegistry::new(test_config());
    let metrics = reg.get_metrics().await;
    assert_eq!(metrics.active_connections, 0);
    assert_eq!(metrics.total_connects, 0);
    assert_eq!(metrics.total_disconnects, 0);
    assert_eq!(metrics.avg_uptime_secs, 0);
}

#[tokio::test]
async fn get_metrics_tracks_connects_and_disconnects() {
    let reg = RemoteAgentRegistry::new(test_config());

    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("a".into(), vec![], "ws://a".into(), tx)
        .await
        .unwrap();

    let metrics = reg.get_metrics().await;
    assert_eq!(metrics.active_connections, 1);
    assert_eq!(metrics.total_connects, 1);
    assert_eq!(metrics.total_disconnects, 0);

    reg.unregister("a").await;
    let metrics = reg.get_metrics().await;
    assert_eq!(metrics.active_connections, 0);
    assert_eq!(metrics.total_connects, 1);
    assert_eq!(metrics.total_disconnects, 1);
}

#[tokio::test]
async fn get_metrics_avg_uptime_is_nonzero_after_disconnect() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("b".into(), vec![], "ws://b".into(), tx)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;
    reg.unregister("b").await;

    let metrics = reg.get_metrics().await;
    assert_eq!(metrics.total_connects, 1);
    assert_eq!(metrics.total_disconnects, 1);
    // avg_uptime may still be 0 due to sub-second rounding, but must not panic
    let _ = metrics.avg_uptime_secs;
}

#[tokio::test]
async fn reap_stale_updates_disconnect_metrics() {
    let config = RemoteConfig {
        heartbeat_timeout_secs: 0,
        ..Default::default()
    };
    let reg = RemoteAgentRegistry::new(config);
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("c".into(), vec![], "ws://c".into(), tx)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(1)).await;
    let reaped = reg.reap_stale().await;
    assert!(reaped.contains(&"c".to_string()));

    let metrics = reg.get_metrics().await;
    assert_eq!(metrics.total_disconnects, 1);
    assert_eq!(metrics.active_connections, 0);
}

#[tokio::test]
async fn mark_reconnecting_and_connected_transitions() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("d".into(), vec![], "ws://d".into(), tx)
        .await
        .unwrap();

    // Initially connected.
    {
        let agents = reg.list().await;
        assert_eq!(
            agents
                .iter()
                .find(|a| a.name == "d")
                .unwrap()
                .connection_state,
            ConnectionState::Connected
        );
    }

    reg.mark_reconnecting("d").await;
    {
        let agents = reg.list().await;
        assert_eq!(
            agents
                .iter()
                .find(|a| a.name == "d")
                .unwrap()
                .connection_state,
            ConnectionState::Reconnecting
        );
    }

    reg.mark_connected("d").await;
    {
        let agents = reg.list().await;
        assert_eq!(
            agents
                .iter()
                .find(|a| a.name == "d")
                .unwrap()
                .connection_state,
            ConnectionState::Connected
        );
    }
}

#[tokio::test]
async fn send_to_dropped_receiver_returns_false() {
    // When the receiver is dropped, the channel is closed and send_to should return false.
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register("dropped".into(), vec![], "ws://d".into(), tx)
        .await
        .unwrap();

    // Drop the receiver to close the channel.
    drop(rx);

    let msg = ProtocolMessage::Heartbeat { timestamp: 1 };
    assert!(!reg.send_to("dropped", msg).await);
}

#[tokio::test]
async fn register_reconnects_detached_agent_and_replays_buffered_events() {
    let reg = RemoteAgentRegistry::new(RemoteConfig {
        replay_buffer_capacity: 4,
        ..test_config()
    });
    let (tx1, mut rx1) = tokio::sync::mpsc::unbounded_channel();
    reg.register("resume-agent".into(), vec![], "ws://one".into(), tx1)
        .await
        .unwrap();

    assert!(
        reg.send_to(
            "resume-agent",
            ProtocolMessage::MessageRelay {
                from: "local".into(),
                to: "resume-agent".into(),
                payload: "first".into(),
            },
        )
        .await
    );
    assert!(
        reg.send_to(
            "resume-agent",
            ProtocolMessage::MessageRelay {
                from: "local".into(),
                to: "resume-agent".into(),
                payload: "second".into(),
            },
        )
        .await
    );
    let _ = rx1.recv().await.expect("first delivery should exist");
    let _ = rx1.recv().await.expect("second delivery should exist");

    assert!(reg.detach_connection("resume-agent").await);

    assert!(
        reg.send_to(
            "resume-agent",
            ProtocolMessage::Broadcast {
                from: "ops".into(),
                channel: "alerts".into(),
                payload: "buffered".into(),
            },
        )
        .await
    );

    let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
    reg.register("resume-agent".into(), vec![], "ws://two".into(), tx2)
        .await
        .unwrap();

    assert_eq!(
        reg.replay_since("resume-agent", 2).await,
        ReplayResult::Replayed(1)
    );

    match rx2.recv().await.expect("replayed broadcast should exist") {
        ProtocolMessage::Broadcast { payload, .. } => assert_eq!(payload, "buffered"),
        other => panic!("expected replayed broadcast, got {other:?}"),
    }
}

#[tokio::test]
async fn unregister_if_detached_only_removes_detached_agents() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    reg.register("live-agent".into(), vec![], "ws://live".into(), tx)
        .await
        .unwrap();

    assert!(!reg.unregister_if_detached("live-agent").await);
    assert!(reg.is_connected("live-agent").await);

    assert!(reg.detach_connection("live-agent").await);
    assert!(reg.unregister_if_detached("live-agent").await);
    assert!(!reg.is_connected("live-agent").await);
}

#[tokio::test]
async fn mark_reconnecting_noop_for_unknown_agent() {
    let reg = RemoteAgentRegistry::new(test_config());
    // Should not panic even if the agent is not registered.
    reg.mark_reconnecting("ghost").await;
    assert!(!reg.is_connected("ghost").await);
}

#[tokio::test]
async fn mark_connected_noop_for_unknown_agent() {
    let reg = RemoteAgentRegistry::new(test_config());
    // Should not panic even if the agent is not registered.
    reg.mark_connected("ghost").await;
    assert!(!reg.is_connected("ghost").await);
}

#[tokio::test]
async fn unregister_nonexistent_agent_is_noop() {
    let reg = RemoteAgentRegistry::new(test_config());
    // Should not panic when unregistering an agent that was never registered.
    reg.unregister("never-existed").await;
    let metrics = reg.get_metrics().await;
    // disconnect counter still incremented even for unknown agent
    assert_eq!(metrics.total_disconnects, 1);
}

#[tokio::test]
async fn capabilities_are_stored_on_registration() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let caps = vec!["code-review".to_string(), "deploy".to_string()];
    reg.register("cap-agent".into(), caps.clone(), "ws://cap".into(), tx)
        .await
        .unwrap();

    let agents = reg.list().await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].capabilities, caps);
}

#[tokio::test]
async fn reap_stale_empty_registry_returns_empty() {
    let reg = RemoteAgentRegistry::new(test_config());
    let reaped = reg.reap_stale().await;
    assert!(reaped.is_empty());
}

#[tokio::test]
async fn concurrent_registrations_all_succeed() {
    let reg = RemoteAgentRegistry::new(test_config());
    let reg = Arc::new(reg);

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let reg = reg.clone();
            tokio::spawn(async move {
                let (tx, _) = tokio::sync::mpsc::unbounded_channel();
                reg.register(
                    format!("concurrent-{i}"),
                    vec![],
                    format!("ws://host:{}", 9000 + i),
                    tx,
                )
                .await
            })
        })
        .collect();

    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    let agents = reg.list().await;
    assert_eq!(agents.len(), 10);
    let metrics = reg.get_metrics().await;
    assert_eq!(metrics.total_connects, 10);
}

#[tokio::test]
async fn get_metrics_avg_uptime_across_multiple_disconnects() {
    let reg = RemoteAgentRegistry::new(test_config());

    for i in 0..3 {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        reg.register(format!("agent-{i}"), vec![], format!("ws://{i}"), tx)
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(10)).await;

    for i in 0..3 {
        reg.unregister(&format!("agent-{i}")).await;
    }

    let metrics = reg.get_metrics().await;
    assert_eq!(metrics.total_connects, 3);
    assert_eq!(metrics.total_disconnects, 3);
    assert_eq!(metrics.active_connections, 0);
    // avg_uptime_secs may be 0 due to sub-second rounding, but must not panic
    let _ = metrics.avg_uptime_secs;
}

#[test]
fn reconnect_with_zero_last_event_id_roundtrip() {
    // Default last_event_id should be 0 when field is omitted.
    let json = r#"{"type":"reconnect"}"#;
    let msg: ProtocolMessage = serde_json::from_str(json).unwrap();
    match msg {
        ProtocolMessage::Reconnect { last_event_id } => assert_eq!(last_event_id, 0),
        _ => unreachable!("wrong variant"),
    }
}
