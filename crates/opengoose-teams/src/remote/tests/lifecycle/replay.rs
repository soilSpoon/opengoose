use std::time::Duration;

use crate::remote::protocol::ProtocolMessage;
use crate::remote::registry::{RemoteAgentRegistry, RemoteConfig};
use crate::remote::transport::ReplayResult;

use super::super::test_config;

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
