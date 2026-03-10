use axum::Json;
use axum::extract::State;
use serde::Serialize;

use opengoose_types::ChannelMetricsSnapshot;

use super::AppError;
use crate::state::AppState;

/// JSON response for the live per-platform channel connection metrics.
#[derive(Serialize)]
pub struct ChannelMetricsResponse {
    pub platforms: std::collections::HashMap<String, ChannelMetricsSnapshot>,
}

/// GET /api/channel-metrics — return the in-memory channel adapter metrics snapshot.
pub async fn get_channel_metrics(
    State(state): State<AppState>,
) -> Result<Json<ChannelMetricsResponse>, AppError> {
    Ok(Json(ChannelMetricsResponse {
        platforms: state.channel_metrics.snapshot(),
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Json;
    use axum::extract::State;
    use opengoose_persistence::{
        AlertStore, Database, OrchestrationStore, ScheduleStore, SessionStore, TriggerStore,
    };
    use opengoose_profiles::ProfileStore;
    use opengoose_teams::TeamStore;
    use opengoose_types::ChannelMetricsStore;

    use super::get_channel_metrics;
    use crate::state::AppState;

    fn make_state_with_metrics(metrics: ChannelMetricsStore) -> AppState {
        let db = Arc::new(Database::open_in_memory().expect("in-memory db should open"));
        AppState {
            db: db.clone(),
            session_store: Arc::new(SessionStore::new(db.clone())),
            orchestration_store: Arc::new(OrchestrationStore::new(db.clone())),
            profile_store: Arc::new(ProfileStore::with_dir(
                std::env::temp_dir().join("ch-metrics-profiles"),
            )),
            team_store: Arc::new(TeamStore::with_dir(
                std::env::temp_dir().join("ch-metrics-teams"),
            )),
            schedule_store: Arc::new(ScheduleStore::new(db.clone())),
            trigger_store: Arc::new(TriggerStore::new(db.clone())),
            alert_store: Arc::new(AlertStore::new(db)),
            channel_metrics: metrics,
        }
    }

    #[tokio::test]
    async fn empty_metrics_returns_empty_platforms_map() {
        let state = make_state_with_metrics(ChannelMetricsStore::new());
        let Json(resp) = get_channel_metrics(State(state))
            .await
            .expect("handler should succeed");
        assert!(resp.platforms.is_empty());
    }

    #[tokio::test]
    async fn connected_platform_appears_in_response() {
        let metrics = ChannelMetricsStore::new();
        metrics.set_connected("discord");
        let state = make_state_with_metrics(metrics);

        let Json(resp) = get_channel_metrics(State(state))
            .await
            .expect("handler should succeed");

        assert!(resp.platforms.contains_key("discord"));
        let discord = &resp.platforms["discord"];
        assert!(discord.uptime_secs.is_some());
        assert_eq!(discord.reconnect_count, 0);
        assert!(discord.last_error.is_none());
    }

    #[tokio::test]
    async fn reconnect_error_appears_in_response() {
        let metrics = ChannelMetricsStore::new();
        metrics.record_reconnect("slack", Some("connection refused".into()));
        metrics.record_reconnect("slack", None);
        let state = make_state_with_metrics(metrics);

        let Json(resp) = get_channel_metrics(State(state))
            .await
            .expect("handler should succeed");

        let slack = &resp.platforms["slack"];
        assert_eq!(slack.reconnect_count, 2);
        assert_eq!(slack.last_error.as_deref(), Some("connection refused"));
        assert!(slack.uptime_secs.is_none());
    }

    #[tokio::test]
    async fn multiple_platforms_all_returned() {
        let metrics = ChannelMetricsStore::new();
        metrics.set_connected("discord");
        metrics.record_reconnect("matrix", Some("timeout".into()));
        let state = make_state_with_metrics(metrics);

        let Json(resp) = get_channel_metrics(State(state))
            .await
            .expect("handler should succeed");

        assert_eq!(resp.platforms.len(), 2);
        assert!(resp.platforms.contains_key("discord"));
        assert!(resp.platforms.contains_key("matrix"));
    }
}
