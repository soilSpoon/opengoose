use std::collections::HashSet;

use opengoose_types::AppEventKind;
use serde::Serialize;

use crate::handlers::AppError;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum LiveEventType {
    Dashboard,
    Session,
    Run,
    Queue,
    Channel,
    Error,
}

impl LiveEventType {
    pub(super) fn as_str(self) -> &'static str {
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
pub(super) struct EventFilter {
    allowed: Option<HashSet<LiveEventType>>,
}

impl EventFilter {
    fn matches(&self, event_type: LiveEventType) -> bool {
        self.allowed
            .as_ref()
            .is_none_or(|allowed| allowed.contains(&event_type))
    }

    pub(super) fn parse(raw: Option<&str>) -> Result<Self, AppError> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SerializedEvent {
    pub(super) event: LiveEventType,
    pub(super) data: String,
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

pub(super) fn serialize_app_event(
    kind: &AppEventKind,
    filter: &EventFilter,
) -> Option<SerializedEvent> {
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
        | AppEventKind::StreamCompleted { session_key, .. }
        | AppEventKind::ModelChanged { session_key, .. }
        | AppEventKind::ContextCompacted { session_key }
        | AppEventKind::ExtensionNotification { session_key, .. } => {
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
        AppEventKind::Error { .. } | AppEventKind::TracingEvent { .. } => (
            LiveEventType::Error,
            LiveEventPayload::new(LiveEventType::Error),
        ),
        AppEventKind::PairingCodeGenerated { .. }
        | AppEventKind::ShutdownStarted { .. }
        | AppEventKind::ShutdownCompleted { .. } => (
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
