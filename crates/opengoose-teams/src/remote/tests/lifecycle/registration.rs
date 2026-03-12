use std::sync::Arc;

use crate::remote::registry::RemoteAgentRegistry;

use super::super::test_config;

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
