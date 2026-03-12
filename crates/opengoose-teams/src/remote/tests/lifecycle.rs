use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::remote::protocol::{ConnectionState, ProtocolMessage};
use crate::remote::registry::{RemoteAgent, RemoteAgentRegistry, RemoteConfig};
use crate::remote::transport::ReplayResult;

use super::test_config;

// --- Registration ---

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

// --- Unregister ---

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
async fn unregister_nonexistent_agent_is_noop() {
    let reg = RemoteAgentRegistry::new(test_config());
    reg.unregister("never-existed").await;
    let metrics = reg.get_metrics().await;
    assert_eq!(metrics.total_disconnects, 1);
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

// --- Staleness & config ---

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

// --- Heartbeat ---

#[tokio::test]
async fn touch_heartbeat_keeps_agent_registered() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("hb-agent".into(), vec![], "ws://hb".into(), tx)
        .await
        .unwrap();

    reg.touch_heartbeat("hb-agent").await;
    assert!(reg.is_connected("hb-agent").await);
}

#[tokio::test]
async fn touch_heartbeat_unknown_agent_is_noop() {
    let reg = RemoteAgentRegistry::new(test_config());
    reg.touch_heartbeat("nonexistent").await;
    assert!(!reg.is_connected("nonexistent").await);
}

// --- Reap stale ---

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

    tokio::time::sleep(Duration::from_millis(1)).await;

    reg.register("just-joined".into(), vec![], "ws://j".into(), tx2)
        .await
        .unwrap();

    let reaped = reg.reap_stale().await;
    assert!(reaped.contains(&"will-reap".to_string()));
    assert!(!reg.is_connected("will-reap").await);
}

#[tokio::test]
async fn reap_stale_empty_registry_returns_empty() {
    let reg = RemoteAgentRegistry::new(test_config());
    let reaped = reg.reap_stale().await;
    assert!(reaped.is_empty());
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

// --- Connection state transitions ---

#[tokio::test]
async fn mark_reconnecting_and_connected_transitions() {
    let reg = RemoteAgentRegistry::new(test_config());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    reg.register("d".into(), vec![], "ws://d".into(), tx)
        .await
        .unwrap();

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
async fn mark_reconnecting_noop_for_unknown_agent() {
    let reg = RemoteAgentRegistry::new(test_config());
    reg.mark_reconnecting("ghost").await;
    assert!(!reg.is_connected("ghost").await);
}

#[tokio::test]
async fn mark_connected_noop_for_unknown_agent() {
    let reg = RemoteAgentRegistry::new(test_config());
    reg.mark_connected("ghost").await;
    assert!(!reg.is_connected("ghost").await);
}

// --- Reconnect with replay ---

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

// --- Metrics ---

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
    let _ = metrics.avg_uptime_secs;
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
    let _ = metrics.avg_uptime_secs;
}

// --- Change subscription ---

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
