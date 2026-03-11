use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use opengoose_teams::remote::{ProtocolMessage, RemoteAgentRegistry, RemoteConfig};

use super::connection::reconnect_ack;
use super::{RemoteGatewayState, disconnect_remote, gateway_health, list_remote};

fn make_state(config: RemoteConfig) -> Arc<RemoteGatewayState> {
    Arc::new(RemoteGatewayState {
        registry: RemoteAgentRegistry::new(config),
    })
}

#[tokio::test]
async fn list_remote_empty_registry() {
    let state = make_state(RemoteConfig::default());
    let axum::Json(agents) = list_remote(State(state)).await;
    assert!(agents.is_empty());
}

#[tokio::test]
async fn list_remote_with_registered_agents() {
    let state = make_state(RemoteConfig::default());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    state
        .registry
        .register(
            "remote-a".into(),
            vec!["cap-x".into()],
            "ws://remote-a:9000".into(),
            tx,
        )
        .await
        .unwrap();

    let axum::Json(agents) = list_remote(State(state)).await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].name, "remote-a");
    assert_eq!(agents[0].capabilities, vec!["cap-x"]);
    assert_eq!(agents[0].endpoint, "ws://remote-a:9000");
}

#[tokio::test]
async fn list_remote_reflects_multiple_agents() {
    let state = make_state(RemoteConfig::default());
    for i in 0..3 {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        state
            .registry
            .register(
                format!("agent-{i}"),
                vec![],
                format!("ws://host:{}", 9000 + i),
                tx,
            )
            .await
            .unwrap();
    }

    let axum::Json(agents) = list_remote(State(state)).await;
    assert_eq!(agents.len(), 3);
}

#[tokio::test]
async fn disconnect_remote_connected_agent_returns_ok() {
    let state = make_state(RemoteConfig::default());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    state
        .registry
        .register("conn-agent".into(), vec![], "ws://c".into(), tx)
        .await
        .unwrap();

    let response = disconnect_remote(State(state.clone()), Path("conn-agent".into()))
        .await
        .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(!state.registry.is_connected("conn-agent").await);
}

#[tokio::test]
async fn disconnect_remote_unknown_agent_returns_not_found() {
    let state = make_state(RemoteConfig::default());
    let response = disconnect_remote(State(state), Path("ghost".into()))
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn disconnect_remote_sends_disconnect_message_to_agent() {
    let state = make_state(RemoteConfig::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .registry
        .register("msg-agent".into(), vec![], "ws://m".into(), tx)
        .await
        .unwrap();

    let _ = disconnect_remote(State(state), Path("msg-agent".into())).await;

    let msg = rx.try_recv().expect("disconnect message should be queued");
    match msg {
        ProtocolMessage::Disconnect { reason } => {
            assert!(reason.contains("server"));
        }
        other => panic!("expected Disconnect, got {:?}", other),
    }
}

#[tokio::test]
async fn disconnect_remote_updates_gateway_metrics() {
    let state = make_state(RemoteConfig::default());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    state
        .registry
        .register("metrics-agent".into(), vec![], "ws://metrics".into(), tx)
        .await
        .unwrap();

    let response = disconnect_remote(State(state.clone()), Path("metrics-agent".into()))
        .await
        .into_response();

    assert_eq!(response.status(), StatusCode::OK);

    let axum::Json(metrics) = gateway_health(State(state)).await;
    assert_eq!(metrics.active_connections, 0);
    assert_eq!(metrics.total_connects, 1);
    assert_eq!(metrics.total_disconnects, 1);
}

#[tokio::test]
async fn reconnect_ack_reports_replayed_event_count() {
    let state = make_state(RemoteConfig {
        replay_buffer_capacity: 4,
        ..RemoteConfig::default()
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .registry
        .register("replay-agent".into(), vec![], "ws://replay".into(), tx)
        .await
        .unwrap();

    assert!(
        state
            .registry
            .send_to(
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
        state
            .registry
            .send_to(
                "replay-agent",
                ProtocolMessage::MessageRelay {
                    from: "local".into(),
                    to: "replay-agent".into(),
                    payload: "second".into(),
                },
            )
            .await
    );

    let _ = rx
        .recv()
        .await
        .expect("initial first delivery should exist");
    let _ = rx
        .recv()
        .await
        .expect("initial second delivery should exist");

    match reconnect_ack(&state.registry, "replay-agent", 1).await {
        ProtocolMessage::ReconnectAck {
            success,
            replayed_events,
        } => {
            assert!(success);
            assert_eq!(replayed_events, 1);
        }
        other => panic!("expected reconnect ack, got {other:?}"),
    }

    match rx.recv().await.expect("replayed delivery should exist") {
        ProtocolMessage::MessageRelay { payload, .. } => assert_eq!(payload, "second"),
        other => panic!("expected replayed relay, got {other:?}"),
    }
}

#[tokio::test]
async fn reconnect_ack_fails_when_replay_window_is_truncated() {
    let state = make_state(RemoteConfig {
        replay_buffer_capacity: 1,
        ..RemoteConfig::default()
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .registry
        .register("window-agent".into(), vec![], "ws://window".into(), tx)
        .await
        .unwrap();

    for payload in ["one", "two"] {
        assert!(
            state
                .registry
                .send_to(
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

    let _ = rx
        .recv()
        .await
        .expect("initial first delivery should exist");
    let _ = rx
        .recv()
        .await
        .expect("initial second delivery should exist");

    match reconnect_ack(&state.registry, "window-agent", 0).await {
        ProtocolMessage::ReconnectAck {
            success,
            replayed_events,
        } => {
            assert!(!success);
            assert_eq!(replayed_events, 0);
        }
        other => panic!("expected reconnect ack, got {other:?}"),
    }
}

#[tokio::test]
async fn reconnect_ack_fails_for_unknown_agent() {
    let state = make_state(RemoteConfig::default());

    match reconnect_ack(&state.registry, "ghost", 0).await {
        ProtocolMessage::ReconnectAck {
            success,
            replayed_events,
        } => {
            assert!(!success);
            assert_eq!(replayed_events, 0);
        }
        other => panic!("expected reconnect ack, got {other:?}"),
    }
}

#[tokio::test]
async fn gateway_health_returns_zero_metrics_when_empty() {
    let state = make_state(RemoteConfig::default());
    let axum::Json(metrics) = gateway_health(State(state)).await;
    assert_eq!(metrics.active_connections, 0);
    assert_eq!(metrics.total_connects, 0);
    assert_eq!(metrics.total_disconnects, 0);
    assert_eq!(metrics.avg_uptime_secs, 0);
}

#[tokio::test]
async fn gateway_health_reflects_active_connection() {
    let state = make_state(RemoteConfig::default());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    state
        .registry
        .register("health-agent".into(), vec![], "ws://h".into(), tx)
        .await
        .unwrap();

    let axum::Json(metrics) = gateway_health(State(state)).await;
    assert_eq!(metrics.active_connections, 1);
    assert_eq!(metrics.total_connects, 1);
    assert_eq!(metrics.total_disconnects, 0);
}

#[tokio::test]
async fn gateway_health_reflects_disconnect() {
    let state = make_state(RemoteConfig::default());
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    state
        .registry
        .register("disco-agent".into(), vec![], "ws://d".into(), tx)
        .await
        .unwrap();
    state.registry.unregister("disco-agent").await;

    let axum::Json(metrics) = gateway_health(State(state)).await;
    assert_eq!(metrics.active_connections, 0);
    assert_eq!(metrics.total_connects, 1);
    assert_eq!(metrics.total_disconnects, 1);
}
