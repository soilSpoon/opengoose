use std::collections::HashSet;
use std::convert::Infallible;
use std::time::Duration;

use async_stream::stream;
use axum::Json;
use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use opengoose_persistence::{EventHistoryQuery, EventStore, normalize_since_filter};
use opengoose_types::AppEventKind;
use serde::{Deserialize, Serialize};

use super::AppError;
use crate::state::AppState;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum LiveEventType {
    Dashboard,
    Session,
    Run,
    Queue,
    Channel,
    Error,
}

impl LiveEventType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Session => "session",
            Self::Run => "run",
            Self::Queue => "queue",
            Self::Channel => "channel",
            Self::Error => "error",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dashboard" => Some(Self::Dashboard),
            "session" => Some(Self::Session),
            "run" => Some(Self::Run),
            "queue" => Some(Self::Queue),
            "channel" => Some(Self::Channel),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    fn supported_values() -> &'static str {
        "dashboard, session, run, queue, channel, error"
    }
}

#[derive(Clone, Debug, Default)]
struct EventFilter {
    allowed: Option<HashSet<LiveEventType>>,
}

impl EventFilter {
    fn matches(&self, event_type: LiveEventType) -> bool {
        self.allowed
            .as_ref()
            .is_none_or(|allowed| allowed.contains(&event_type))
    }

    fn parse(raw: Option<&str>) -> Result<Self, AppError> {
        let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(Self::default());
        };

        let mut allowed = HashSet::new();
        for value in raw.split(',') {
            let event_type = LiveEventType::parse(value).ok_or_else(|| {
                AppError::UnprocessableEntity(format!(
                    "unknown live event type `{}`. Valid: {}",
                    value.trim(),
                    LiveEventType::supported_values()
                ))
            })?;
            allowed.insert(event_type);
        }

        Ok(Self {
            allowed: Some(allowed),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    pub types: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EventHistoryQueryParams {
    #[serde(default = "default_history_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub gateway: Option<String>,
    pub kind: Option<String>,
    pub session_key: Option<String>,
    pub since: Option<String>,
}

fn default_history_limit() -> i64 {
    100
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SerializedEvent {
    event: LiveEventType,
    data: String,
}

impl SerializedEvent {
    fn into_sse_event(self) -> Event {
        Event::default().event(self.event.as_str()).data(self.data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveEventPayload {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EventHistoryResponse {
    pub id: i32,
    pub event_kind: String,
    pub timestamp: String,
    pub source_gateway: Option<String>,
    pub session_key: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct EventHistoryPageResponse {
    pub items: Vec<EventHistoryResponse>,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

impl LiveEventPayload {
    fn new(event_type: LiveEventType) -> Self {
        Self {
            kind: event_type.as_str(),
            session_key: None,
            team_run_id: None,
            status: None,
        }
    }
}

fn serialize_app_event(kind: &AppEventKind, filter: &EventFilter) -> Option<SerializedEvent> {
    let (event_type, mut payload) = match kind {
        AppEventKind::DashboardUpdated => (
            LiveEventType::Dashboard,
            LiveEventPayload::new(LiveEventType::Dashboard),
        ),
        AppEventKind::SessionUpdated { session_key }
        | AppEventKind::MessageReceived { session_key, .. }
        | AppEventKind::ResponseSent { session_key, .. }
        | AppEventKind::PairingCompleted { session_key }
        | AppEventKind::TeamActivated { session_key, .. }
        | AppEventKind::TeamDeactivated { session_key }
        | AppEventKind::SessionDisconnected { session_key, .. }
        | AppEventKind::StreamStarted { session_key, .. }
        | AppEventKind::StreamUpdated { session_key, .. }
        | AppEventKind::StreamCompleted { session_key, .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Session);
            payload.session_key = Some(session_key.to_stable_id());
            (LiveEventType::Session, payload)
        }
        AppEventKind::RunUpdated {
            team_run_id,
            status,
        } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Run);
            payload.team_run_id = Some(team_run_id.clone());
            payload.status = Some(status.clone());
            (LiveEventType::Run, payload)
        }
        AppEventKind::TeamRunStarted { .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Run);
            payload.status = Some("started".into());
            (LiveEventType::Run, payload)
        }
        AppEventKind::TeamStepStarted { .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Run);
            payload.status = Some("step_started".into());
            (LiveEventType::Run, payload)
        }
        AppEventKind::TeamStepCompleted { .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Run);
            payload.status = Some("step_completed".into());
            (LiveEventType::Run, payload)
        }
        AppEventKind::TeamStepFailed { .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Run);
            payload.status = Some("step_failed".into());
            (LiveEventType::Run, payload)
        }
        AppEventKind::TeamRunCompleted { .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Run);
            payload.status = Some("completed".into());
            (LiveEventType::Run, payload)
        }
        AppEventKind::TeamRunFailed { .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Run);
            payload.status = Some("failed".into());
            (LiveEventType::Run, payload)
        }
        AppEventKind::QueueUpdated { team_run_id } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Queue);
            payload.team_run_id = team_run_id.clone();
            (LiveEventType::Queue, payload)
        }
        AppEventKind::GooseReady
        | AppEventKind::ChannelReady { .. }
        | AppEventKind::ChannelDisconnected { .. }
        | AppEventKind::ChannelReconnecting { .. } => (
            LiveEventType::Channel,
            LiveEventPayload::new(LiveEventType::Channel),
        ),
        AppEventKind::AlertFired { .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Channel);
            payload.status = Some("alert_fired".into());
            (LiveEventType::Channel, payload)
        }
        AppEventKind::ShutdownStarted { .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Channel);
            payload.status = Some("shutdown_started".into());
            (LiveEventType::Channel, payload)
        }
        AppEventKind::ShutdownCompleted { timed_out, .. } => {
            let mut payload = LiveEventPayload::new(LiveEventType::Channel);
            payload.status = Some(if *timed_out {
                "shutdown_timed_out".into()
            } else {
                "shutdown_completed".into()
            });
            (LiveEventType::Channel, payload)
        }
        AppEventKind::Error { .. } | AppEventKind::TracingEvent { .. } => (
            LiveEventType::Error,
            LiveEventPayload::new(LiveEventType::Error),
        ),
        AppEventKind::PairingCodeGenerated { .. } => (
            LiveEventType::Channel,
            LiveEventPayload::new(LiveEventType::Channel),
        ),
    };

    if !filter.matches(event_type) {
        return None;
    }

    payload.kind = event_type.as_str();
    let data = serde_json::to_string(&payload).ok()?;
    Some(SerializedEvent {
        event: event_type,
        data,
    })
}

fn build_event_stream(
    mut rx: tokio::sync::broadcast::Receiver<opengoose_types::AppEvent>,
    filter: EventFilter,
) -> impl Stream<Item = Result<Event, Infallible>> + Send {
    stream! {
        loop {
            match rx.recv().await {
                Ok(app_event) => {
                    if let Some(event) = serialize_app_event(&app_event.kind, &filter) {
                        yield Ok(event.into_sse_event());
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

/// GET /api/events — subscribe to live app events as SSE.
pub async fn stream_events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, AppError> {
    let filter = EventFilter::parse(query.types.as_deref())?;
    let event_stream = build_event_stream(state.event_bus.subscribe(), filter);

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("opengoose-events"),
    ))
}

/// GET /api/events/history — list persisted event history with filters.
pub async fn list_event_history(
    State(state): State<AppState>,
    Query(query): Query<EventHistoryQueryParams>,
) -> Result<Json<EventHistoryPageResponse>, AppError> {
    if query.limit <= 0 || query.limit > 1000 {
        return Err(AppError::UnprocessableEntity(format!(
            "`limit` must be between 1 and 1000, got {}",
            query.limit
        )));
    }
    if query.offset < 0 {
        return Err(AppError::UnprocessableEntity(format!(
            "`offset` must be 0 or greater, got {}",
            query.offset
        )));
    }

    let store = EventStore::new(state.db.clone());
    let mut entries = store.list(&EventHistoryQuery {
        limit: query.limit + 1,
        offset: query.offset,
        event_kind: query.kind.clone(),
        source_gateway: query.gateway.clone(),
        session_key: query.session_key.clone(),
        since: query
            .since
            .as_deref()
            .map(normalize_since_filter)
            .transpose()
            .map_err(AppError::UnprocessableEntity)?,
    })?;

    let has_more = entries.len() as i64 > query.limit;
    if has_more {
        entries.truncate(query.limit as usize);
    }

    Ok(Json(EventHistoryPageResponse {
        items: entries
            .into_iter()
            .map(|entry| EventHistoryResponse {
                id: entry.id,
                event_kind: entry.event_kind,
                timestamp: entry.timestamp,
                source_gateway: entry.source_gateway,
                session_key: entry.session_key,
                payload: entry.payload,
            })
            .collect(),
        limit: query.limit,
        offset: query.offset,
        has_more,
    }))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::Json;
    use axum::extract::{Query, State};
    use futures_util::StreamExt;
    use opengoose_persistence::EventStore;
    use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};
    use tokio::time::timeout;

    use super::{
        EventFilter, EventHistoryQueryParams, LiveEventType, build_event_stream,
        list_event_history, serialize_app_event,
    };
    use crate::handlers::test_support::make_state;

    #[test]
    fn session_event_serializes_expected_payload() {
        let serialized = serialize_app_event(
            &AppEventKind::SessionUpdated {
                session_key: SessionKey::from_stable_id("discord:ns:ops:bridge"),
            },
            &EventFilter::default(),
        )
        .expect("session event should serialize");

        assert_eq!(serialized.event, LiveEventType::Session);
        assert_eq!(
            serialized.data,
            r#"{"type":"session","sessionKey":"discord:ns:ops:bridge"}"#
        );
    }

    #[test]
    fn filter_excludes_non_matching_event_types() {
        let filter = EventFilter::parse(Some("run")).expect("filter should parse");

        let serialized = serialize_app_event(
            &AppEventKind::ChannelReady {
                platform: Platform::Slack,
            },
            &filter,
        );

        assert!(serialized.is_none());
    }

    #[tokio::test]
    async fn event_stream_finishes_cleanly_when_bus_closes() {
        let bus = EventBus::new(8);
        let stream = build_event_stream(bus.subscribe(), EventFilter::default());
        tokio::pin!(stream);

        drop(bus);

        let next = timeout(Duration::from_millis(100), stream.next())
            .await
            .expect("stream should stop promptly");

        assert!(next.is_none());
    }

    #[tokio::test]
    async fn list_event_history_returns_persisted_entries() {
        let state = make_state();
        let store = EventStore::new(state.db.clone());
        store
            .record(&AppEventKind::ChannelReady {
                platform: Platform::Discord,
            })
            .expect("event should persist");

        let Json(page) = list_event_history(
            State(state),
            Query(EventHistoryQueryParams {
                limit: 10,
                offset: 0,
                gateway: Some("discord".into()),
                kind: Some("channel_ready".into()),
                session_key: None,
                since: None,
            }),
        )
        .await
        .expect("history query should succeed");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].event_kind, "channel_ready");
        assert_eq!(page.items[0].source_gateway.as_deref(), Some("discord"));
        assert!(!page.has_more);
    }

    #[tokio::test]
    async fn list_event_history_rejects_invalid_limit() {
        let state = make_state();
        let result = list_event_history(
            State(state),
            Query(EventHistoryQueryParams {
                limit: 0,
                offset: 0,
                gateway: None,
                kind: None,
                session_key: None,
                since: None,
            }),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_event_history_rejects_invalid_since_filter() {
        let state = make_state();
        let result = list_event_history(
            State(state),
            Query(EventHistoryQueryParams {
                limit: 10,
                offset: 0,
                gateway: None,
                kind: None,
                session_key: None,
                since: Some("definitely-not-a-time".into()),
            }),
        )
        .await;

        assert!(result.is_err());
    }
}
