//! Trigger type definitions and condition structs.

use serde::{Deserialize, Serialize};

/// The kinds of events a trigger can react to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// React to a file-system change (path glob pattern).
    FileWatch,
    /// React to an inter-agent message matching criteria.
    MessageReceived,
    /// React when a cron schedule completes a run.
    ScheduleComplete,
    /// React to an inbound webhook at a given path.
    WebhookReceived,
    /// React when an end-user message arrives on any channel (`AppEventKind::MessageReceived`).
    OnMessage,
    /// React when a new session starts (`AppEventKind::GooseReady` or `ChannelReady`).
    OnSessionStart,
    /// React when a session disconnects (`AppEventKind::SessionDisconnected`).
    OnSessionEnd,
    /// React when a team run completes (`AppEventKind::TeamRunCompleted`).
    OnSchedule,
}

impl TriggerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileWatch => "file_watch",
            Self::MessageReceived => "message_received",
            Self::ScheduleComplete => "schedule_complete",
            Self::WebhookReceived => "webhook_received",
            Self::OnMessage => "on_message",
            Self::OnSessionStart => "on_session_start",
            Self::OnSessionEnd => "on_session_end",
            Self::OnSchedule => "on_schedule",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "file_watch" => Some(Self::FileWatch),
            "message_received" => Some(Self::MessageReceived),
            "schedule_complete" => Some(Self::ScheduleComplete),
            "webhook_received" => Some(Self::WebhookReceived),
            "on_message" => Some(Self::OnMessage),
            "on_session_start" => Some(Self::OnSessionStart),
            "on_session_end" => Some(Self::OnSessionEnd),
            "on_schedule" => Some(Self::OnSchedule),
            _ => None,
        }
    }

    /// All known trigger type names.
    pub fn all_names() -> &'static [&'static str] {
        &[
            "file_watch",
            "message_received",
            "schedule_complete",
            "webhook_received",
            "on_message",
            "on_session_start",
            "on_session_end",
            "on_schedule",
        ]
    }
}

/// Condition for a `MessageReceived` trigger.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageCondition {
    /// If set, only match messages from this agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_agent: Option<String>,
    /// If set, only match messages on this channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// If set, payload must contain this substring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_contains: Option<String>,
}

/// Condition for a `FileWatch` trigger.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileWatchCondition {
    /// Glob pattern for file paths (e.g. `src/**/*.rs`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// Condition for a `WebhookReceived` trigger.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebhookCondition {
    /// URL path prefix to match (e.g. `/github/pr`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional secret that the caller must supply via `X-Webhook-Secret` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Optional HMAC secret for validating `timestamp.body` with SHA-256.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hmac_secret: Option<String>,
    /// Optional signature header name. Defaults to `X-Webhook-Signature`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_header: Option<String>,
    /// Optional timestamp header name. Defaults to `X-Webhook-Timestamp`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_header: Option<String>,
    /// Optional replay window in seconds. Defaults to 300.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_tolerance_secs: Option<i64>,
    /// Optional per-trigger max requests per rate limit window.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "rate_limit_max_requests"
    )]
    pub rate_limit: Option<u64>,
    /// Optional rate limit window in seconds. Defaults to 60.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "rate_limit_window_seconds"
    )]
    pub rate_limit_window_secs: Option<u64>,
}

/// Condition for a `ScheduleComplete` trigger.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScheduleCompleteCondition {
    /// Schedule name that must complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_name: Option<String>,
}

/// Condition for an `OnMessage` trigger (matches `AppEventKind::MessageReceived`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnMessageCondition {
    /// If set, only match messages from this author.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_author: Option<String>,
    /// If set, message content must contain this substring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_contains: Option<String>,
}

/// Condition for `OnSessionStart` / `OnSessionEnd` triggers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnSessionCondition {
    /// If set, only match sessions on this platform (e.g. "discord", "slack").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

/// Condition for an `OnSchedule` trigger (matches `AppEventKind::TeamRunCompleted`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnScheduleCondition {
    /// If set, only fire when this team name completes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
}

/// Validate a trigger type string.
pub fn validate_trigger_type(s: &str) -> Result<TriggerType, String> {
    TriggerType::parse(s).ok_or_else(|| {
        format!(
            "unknown trigger type '{}'. Valid types: {}",
            s,
            TriggerType::all_names().join(", ")
        )
    })
}
