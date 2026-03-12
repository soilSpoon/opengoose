use std::path::Path;

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
}

/// Check whether a `WebhookReceived` trigger condition matches the given path.
///
/// The path from the trigger condition is treated as a prefix. A trigger with
/// no path configured matches every incoming webhook path.
pub fn matches_webhook_path(condition_json: &str, path: &str) -> bool {
    let cond: WebhookCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };
    match cond.path {
        None => true,
        Some(ref p) => path.starts_with(p.as_str()),
    }
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

/// Check whether an `OnMessage` trigger matches a `MessageReceived` event.
pub fn matches_on_message_event(condition_json: &str, author: &str, content: &str) -> bool {
    let cond: OnMessageCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref expected) = cond.from_author
        && expected != author
    {
        return false;
    }

    if let Some(ref needle) = cond.content_contains
        && !content.contains(needle.as_str())
    {
        return false;
    }

    true
}

/// Check whether an `OnSessionStart`/`OnSessionEnd` trigger matches.
pub fn matches_on_session_event(condition_json: &str, platform: &str) -> bool {
    let cond: OnSessionCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref expected) = cond.platform
        && expected != platform
    {
        return false;
    }

    true
}

/// Check whether an `OnSchedule` trigger matches a `TeamRunCompleted` event.
pub fn matches_on_schedule_event(condition_json: &str, completed_team: &str) -> bool {
    let cond: OnScheduleCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref expected) = cond.team
        && expected != completed_team
    {
        return false;
    }

    true
}

/// Check whether a `FileWatch` condition matches a file path.
///
/// An empty pattern (or missing condition) matches all paths.
pub fn matches_file_watch_event(condition_json: &str, path: &str) -> bool {
    let cond: FileWatchCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let pattern = match &cond.pattern {
        Some(p) if !p.is_empty() => p,
        None => return true,
        Some(_) => return true,
    };

    let glob = match globset::Glob::new(pattern) {
        Ok(g) => g.compile_matcher(),
        Err(_) => return false,
    };

    glob.is_match(Path::new(path))
}

/// Check whether a `MessageReceived` trigger matches a bus event.
pub fn matches_message_event(
    condition_json: &str,
    from: &str,
    channel: Option<&str>,
    payload: &str,
) -> bool {
    let cond: MessageCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref expected_from) = cond.from_agent
        && expected_from != from
    {
        return false;
    }

    if let Some(ref expected_channel) = cond.channel {
        match channel {
            Some(ch) if ch == expected_channel => {}
            _ => return false,
        }
    }

    if let Some(ref needle) = cond.payload_contains
        && !payload.contains(needle.as_str())
    {
        return false;
    }

    true
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
