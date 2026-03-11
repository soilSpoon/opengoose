use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;

use opengoose_types::ChannelMetricsSnapshot;

use super::AppError;
use crate::state::AppState;

/// Known gateway platforms in the system.
const KNOWN_PLATFORMS: &[&str] = &["discord", "slack", "telegram", "matrix"];

/// Derived connection state from metrics data.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
}

/// Summary of a single gateway platform.
#[derive(Debug, Clone, Serialize)]
pub struct GatewaySummary {
    pub platform: String,
    pub state: ConnectionState,
    pub uptime_secs: Option<u64>,
    pub reconnect_count: u32,
    pub last_error: Option<String>,
}

/// Response for GET /api/gateways.
#[derive(Serialize)]
pub struct GatewayListResponse {
    pub gateways: Vec<GatewaySummary>,
}

/// Response for GET /api/gateways/{platform}/status.
#[derive(Serialize)]
pub struct GatewayStatusResponse {
    pub platform: String,
    pub state: ConnectionState,
    pub uptime_secs: Option<u64>,
    pub reconnect_count: u32,
    pub last_error: Option<String>,
}

/// Derive connection state from a metrics snapshot.
fn derive_state(snapshot: &ChannelMetricsSnapshot) -> ConnectionState {
    if snapshot.uptime_secs.is_some() {
        ConnectionState::Connected
    } else if snapshot.reconnect_count > 0 {
        ConnectionState::Reconnecting
    } else {
        ConnectionState::Disconnected
    }
}

fn build_summary(platform: &str, snapshot: Option<&ChannelMetricsSnapshot>) -> GatewaySummary {
    match snapshot {
        Some(snap) => GatewaySummary {
            platform: platform.to_string(),
            state: derive_state(snap),
            uptime_secs: snap.uptime_secs,
            reconnect_count: snap.reconnect_count,
            last_error: snap.last_error.clone(),
        },
        None => GatewaySummary {
            platform: platform.to_string(),
            state: ConnectionState::Disconnected,
            uptime_secs: None,
            reconnect_count: 0,
            last_error: None,
        },
    }
}

/// GET /api/gateways — return connection status for all known gateway platforms.
pub async fn list_gateways(
    State(state): State<AppState>,
) -> Result<Json<GatewayListResponse>, AppError> {
    let snapshots = state.channel_metrics.snapshot();
    let mut gateways: Vec<GatewaySummary> = KNOWN_PLATFORMS
        .iter()
        .map(|platform| build_summary(platform, snapshots.get(*platform)))
        .collect();

    // Include any platforms in metrics that aren't in KNOWN_PLATFORMS.
    for (platform, snap) in &snapshots {
        if !KNOWN_PLATFORMS.contains(&platform.as_str()) {
            gateways.push(build_summary(platform, Some(snap)));
        }
    }

    Ok(Json(GatewayListResponse { gateways }))
}

/// GET /api/gateways/{platform}/status — return detailed health for one platform.
pub async fn gateway_status(
    State(state): State<AppState>,
    Path(platform): Path<String>,
) -> Result<Json<GatewayStatusResponse>, AppError> {
    let snapshots = state.channel_metrics.snapshot();
    let snapshot = snapshots.get(&platform);

    // Return status even for unknown platforms (they'll show as disconnected).
    let (connection_state, uptime, reconnects, error) = match snapshot {
        Some(snap) => (
            derive_state(snap),
            snap.uptime_secs,
            snap.reconnect_count,
            snap.last_error.clone(),
        ),
        None => (ConnectionState::Disconnected, None, 0, None),
    };

    Ok(Json(GatewayStatusResponse {
        platform,
        state: connection_state,
        uptime_secs: uptime,
        reconnect_count: reconnects,
        last_error: error,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Json;
    use axum::extract::{Path, State};
    use opengoose_persistence::{
        AlertStore, ApiKeyStore, Database, OrchestrationStore, ScheduleStore, SessionStore,
        TriggerStore,
    };
    use opengoose_profiles::ProfileStore;
    use opengoose_teams::TeamStore;
    use opengoose_types::{ChannelMetricsStore, EventBus};

    use super::*;
    use crate::state::AppState;

    fn make_state(metrics: ChannelMetricsStore) -> AppState {
        let db = Arc::new(Database::open_in_memory().expect("in-memory db"));
        AppState {
            db: db.clone(),
            session_store: Arc::new(SessionStore::new(db.clone())),
            orchestration_store: Arc::new(OrchestrationStore::new(db.clone())),
            profile_store: Arc::new(ProfileStore::with_dir(
                std::env::temp_dir().join("gw-handler-profiles"),
            )),
            team_store: Arc::new(TeamStore::with_dir(
                std::env::temp_dir().join("gw-handler-teams"),
            )),
            schedule_store: Arc::new(ScheduleStore::new(db.clone())),
            trigger_store: Arc::new(TriggerStore::new(db.clone())),
            alert_store: Arc::new(AlertStore::new(db.clone())),
            api_key_store: Arc::new(ApiKeyStore::new(db)),
            channel_metrics: metrics,
            event_bus: EventBus::new(256),
        }
    }

    #[tokio::test]
    async fn list_gateways_returns_all_known_platforms_when_empty() {
        let state = make_state(ChannelMetricsStore::new());
        let Json(resp) = list_gateways(State(state)).await.expect("should succeed");
        assert_eq!(resp.gateways.len(), 4);
        let names: Vec<&str> = resp.gateways.iter().map(|g| g.platform.as_str()).collect();
        assert!(names.contains(&"discord"));
        assert!(names.contains(&"slack"));
        assert!(names.contains(&"telegram"));
        assert!(names.contains(&"matrix"));
        for gw in &resp.gateways {
            assert_eq!(gw.state, ConnectionState::Disconnected);
        }
    }

    #[tokio::test]
    async fn list_gateways_shows_connected_platform() {
        let metrics = ChannelMetricsStore::new();
        metrics.set_connected("slack");
        let state = make_state(metrics);
        let Json(resp) = list_gateways(State(state)).await.expect("should succeed");
        let slack = resp
            .gateways
            .iter()
            .find(|g| g.platform == "slack")
            .unwrap();
        assert_eq!(slack.state, ConnectionState::Connected);
        assert!(slack.uptime_secs.is_some());
    }

    #[tokio::test]
    async fn list_gateways_shows_reconnecting_platform() {
        let metrics = ChannelMetricsStore::new();
        metrics.record_reconnect("discord", Some("timeout".into()));
        let state = make_state(metrics);
        let Json(resp) = list_gateways(State(state)).await.expect("should succeed");
        let discord = resp
            .gateways
            .iter()
            .find(|g| g.platform == "discord")
            .unwrap();
        assert_eq!(discord.state, ConnectionState::Reconnecting);
        assert_eq!(discord.last_error.as_deref(), Some("timeout"));
    }

    #[tokio::test]
    async fn list_gateways_includes_unknown_platform() {
        let metrics = ChannelMetricsStore::new();
        metrics.set_connected("irc");
        let state = make_state(metrics);
        let Json(resp) = list_gateways(State(state)).await.expect("should succeed");
        assert_eq!(resp.gateways.len(), 5);
        let irc = resp.gateways.iter().find(|g| g.platform == "irc").unwrap();
        assert_eq!(irc.state, ConnectionState::Connected);
    }

    #[tokio::test]
    async fn gateway_status_returns_connected() {
        let metrics = ChannelMetricsStore::new();
        metrics.set_connected("matrix");
        let state = make_state(metrics);
        let Json(resp) = gateway_status(State(state), Path("matrix".into()))
            .await
            .expect("should succeed");
        assert_eq!(resp.platform, "matrix");
        assert_eq!(resp.state, ConnectionState::Connected);
        assert!(resp.uptime_secs.is_some());
    }

    #[tokio::test]
    async fn gateway_status_returns_disconnected_for_unknown() {
        let state = make_state(ChannelMetricsStore::new());
        let Json(resp) = gateway_status(State(state), Path("foobar".into()))
            .await
            .expect("should succeed");
        assert_eq!(resp.platform, "foobar");
        assert_eq!(resp.state, ConnectionState::Disconnected);
        assert!(resp.uptime_secs.is_none());
    }

    #[tokio::test]
    async fn gateway_status_shows_reconnect_details() {
        let metrics = ChannelMetricsStore::new();
        metrics.record_reconnect("telegram", Some("connection refused".into()));
        metrics.record_reconnect("telegram", None);
        let state = make_state(metrics);
        let Json(resp) = gateway_status(State(state), Path("telegram".into()))
            .await
            .expect("should succeed");
        assert_eq!(resp.state, ConnectionState::Reconnecting);
        assert_eq!(resp.reconnect_count, 2);
        assert_eq!(resp.last_error.as_deref(), Some("connection refused"));
    }

    #[tokio::test]
    async fn derive_state_connected_after_reconnect() {
        let metrics = ChannelMetricsStore::new();
        metrics.record_reconnect("slack", Some("err".into()));
        metrics.set_connected("slack");
        let state = make_state(metrics);
        let Json(resp) = gateway_status(State(state), Path("slack".into()))
            .await
            .expect("should succeed");
        assert_eq!(resp.state, ConnectionState::Connected);
        assert!(resp.last_error.is_none());
    }
}
