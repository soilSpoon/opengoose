use std::time::{Duration, Instant};

use crate::remote::protocol::ConnectionState;
use crate::remote::registry::{RemoteAgent, RemoteAgentRegistry, RemoteConfig};

use super::super::test_config;

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
