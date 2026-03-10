use std::sync::Arc;

use opengoose_persistence::{
    AlertStore, Database, OrchestrationStore, ScheduleStore, SessionStore, TriggerStore,
};
use opengoose_profiles::ProfileStore;
use opengoose_teams::TeamStore;
use opengoose_types::{ChannelMetricsStore, EventBus};

/// Shared application state passed to all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Underlying SQLite database handle.
    pub db: Arc<Database>,
    /// Store for chat sessions and message history.
    pub session_store: Arc<SessionStore>,
    /// Store for team orchestration runs.
    pub orchestration_store: Arc<OrchestrationStore>,
    /// Store for agent profile YAML definitions.
    pub profile_store: Arc<ProfileStore>,
    /// Store for team YAML definitions.
    pub team_store: Arc<TeamStore>,
    /// Store for cron workflow schedules.
    pub schedule_store: Arc<ScheduleStore>,
    /// Store for event-driven workflow triggers.
    pub trigger_store: Arc<TriggerStore>,
    /// Store for monitoring alert rules and history.
    pub alert_store: Arc<AlertStore>,
    /// Live connection metrics from channel adapters (Discord, Slack, Matrix, …).
    /// Shared via `Arc` with the gateway runtime so metrics are updated in real time.
    pub channel_metrics: ChannelMetricsStore,
    /// Shared event bus for live SSE updates in the web UI.
    pub event_bus: EventBus,
}

impl AppState {
    /// Create AppState from an existing shared Database with no channel metrics.
    pub fn new(db: Arc<Database>) -> anyhow::Result<Self> {
        Self::with_metrics_and_events(db, ChannelMetricsStore::new(), EventBus::new(256))
    }

    /// Create AppState with a pre-built `ChannelMetricsStore` shared with the gateway runtime.
    pub fn with_metrics(
        db: Arc<Database>,
        channel_metrics: ChannelMetricsStore,
    ) -> anyhow::Result<Self> {
        Self::with_metrics_and_events(db, channel_metrics, EventBus::new(256))
    }

    /// Create AppState with pre-built metrics and a live event bus.
    pub fn with_metrics_and_events(
        db: Arc<Database>,
        channel_metrics: ChannelMetricsStore,
        event_bus: EventBus,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            session_store: Arc::new(SessionStore::new(db.clone())),
            orchestration_store: Arc::new(OrchestrationStore::new(db.clone())),
            alert_store: Arc::new(AlertStore::new(db.clone())),
            schedule_store: Arc::new(ScheduleStore::new(db.clone())),
            trigger_store: Arc::new(TriggerStore::new(db.clone())),
            profile_store: Arc::new(ProfileStore::new()?),
            team_store: Arc::new(TeamStore::new()?),
            channel_metrics,
            event_bus,
            db,
        })
    }
}
