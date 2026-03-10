//! Workflow event trigger system.
//!
//! Two watchers are provided:
//!
//! - [`spawn_trigger_watcher`]: listens on the [`MessageBus`] for inter-agent
//!   messages and fires `message_received` triggers.
//! - [`spawn_event_bus_trigger_watcher`]: listens on the [`EventBus`] for
//!   system-level events (`on_message`, `on_session_start`, `on_session_end`,
//!   `on_schedule`) and fires the matching triggers.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use opengoose_persistence::{Database, TriggerStore};
use opengoose_types::EventBus;

use crate::message_bus::MessageBus;

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

/// Spawn the trigger watcher as a background task.
///
/// Listens on the global [`MessageBus`] tap and evaluates enabled
/// `message_received` triggers against each incoming event.
pub fn spawn_trigger_watcher(
    db: Arc<Database>,
    event_bus: EventBus,
    message_bus: MessageBus,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("trigger watcher started");
        let mut rx = message_bus.subscribe_all();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("trigger watcher stopped");
                    break;
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Err(e) = handle_bus_event(&db, &event_bus, &event).await {
                                error!(%e, "trigger watcher: failed handling event");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(n, "trigger watcher: lagged behind, skipped messages");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            info!("trigger watcher: bus closed, exiting");
                            break;
                        }
                    }
                }
            }
        }
    })
}

/// Spawn the EventBus trigger watcher as a background task.
///
/// Subscribes to the [`EventBus`] and evaluates `on_message`,
/// `on_session_start`, `on_session_end`, and `on_schedule` triggers
/// against each system event.
pub fn spawn_event_bus_trigger_watcher(
    db: Arc<Database>,
    event_bus: EventBus,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("event-bus trigger watcher started");
        let mut rx = event_bus.subscribe();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("event-bus trigger watcher stopped");
                    break;
                }
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Err(e) = handle_app_event(&db, &event_bus, &event.kind).await {
                                error!(%e, "event-bus trigger watcher: failed handling event");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(n, "event-bus trigger watcher: lagged, skipped events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            info!("event-bus trigger watcher: bus closed, exiting");
                            break;
                        }
                    }
                }
            }
        }
    })
}

async fn handle_app_event(
    db: &Arc<Database>,
    event_bus: &EventBus,
    kind: &opengoose_types::AppEventKind,
) -> anyhow::Result<()> {
    use opengoose_types::AppEventKind;

    match kind {
        AppEventKind::MessageReceived {
            author, content, ..
        } => {
            fire_matching_triggers(db, event_bus, "on_message", |cond| {
                matches_on_message_event(cond, author, content)
            })
            .await?;
        }
        AppEventKind::GooseReady => {
            fire_matching_triggers(db, event_bus, "on_session_start", |cond| {
                matches_on_session_event(cond, "system")
            })
            .await?;
        }
        AppEventKind::ChannelReady { platform } => {
            fire_matching_triggers(db, event_bus, "on_session_start", |cond| {
                matches_on_session_event(cond, &platform.to_string())
            })
            .await?;
        }
        AppEventKind::SessionDisconnected { session_key, .. } => {
            let platform = session_key.platform.to_string();
            fire_matching_triggers(db, event_bus, "on_session_end", |cond| {
                matches_on_session_event(cond, &platform)
            })
            .await?;
        }
        AppEventKind::TeamRunCompleted { team } => {
            fire_matching_triggers(db, event_bus, "on_schedule", |cond| {
                matches_on_schedule_event(cond, team)
            })
            .await?;
        }
        _ => {}
    }

    Ok(())
}

async fn fire_matching_triggers<F>(
    db: &Arc<Database>,
    event_bus: &EventBus,
    trigger_type: &str,
    matches: F,
) -> anyhow::Result<()>
where
    F: Fn(&str) -> bool,
{
    let store = TriggerStore::new(db.clone());
    let triggers = store.list_by_type(trigger_type)?;

    for trigger in triggers {
        if matches(&trigger.condition_json) {
            info!(
                trigger = %trigger.name,
                team = %trigger.team_name,
                trigger_type,
                "trigger matched: firing team run"
            );

            let input = if trigger.input.is_empty() {
                format!("Triggered by {trigger_type} event")
            } else {
                trigger.input.clone()
            };

            match crate::run_headless(&trigger.team_name, &input, db.clone(), event_bus.clone())
                .await
            {
                Ok((run_id, _)) => {
                    info!(trigger = %trigger.name, run_id, "triggered team run completed");
                }
                Err(e) => {
                    warn!(
                        trigger = %trigger.name,
                        team = %trigger.team_name,
                        %e,
                        "triggered team run failed"
                    );
                }
            }

            if let Err(e) = store.mark_fired(&trigger.name) {
                error!(trigger = %trigger.name, %e, "failed to mark trigger as fired");
            }
        }
    }

    Ok(())
}

async fn handle_bus_event(
    db: &Arc<Database>,
    event_bus: &EventBus,
    event: &crate::message_bus::BusEvent,
) -> anyhow::Result<()> {
    let store = TriggerStore::new(db.clone());
    let triggers = store.list_by_type("message_received")?;

    for trigger in triggers {
        let channel = event.channel.as_deref();
        if matches_message_event(
            &trigger.condition_json,
            &event.from,
            channel,
            &event.payload,
        ) {
            info!(
                trigger = %trigger.name,
                team = %trigger.team_name,
                event_from = %event.from,
                "trigger matched: firing team run"
            );

            let input = if trigger.input.is_empty() {
                format!(
                    "Triggered by event from '{}': {}",
                    event.from,
                    truncate(&event.payload, 200)
                )
            } else {
                trigger.input.clone()
            };

            match crate::run_headless(&trigger.team_name, &input, db.clone(), event_bus.clone())
                .await
            {
                Ok((run_id, _)) => {
                    info!(
                        trigger = %trigger.name,
                        run_id = %run_id,
                        "triggered team run completed"
                    );
                }
                Err(e) => {
                    warn!(
                        trigger = %trigger.name,
                        team = %trigger.team_name,
                        %e,
                        "triggered team run failed"
                    );
                }
            }

            if let Err(e) = store.mark_fired(&trigger.name) {
                error!(trigger = %trigger.name, %e, "failed to mark trigger as fired");
            }
        }
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..s.floor_char_boundary(max)]
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_type_roundtrip() {
        for name in TriggerType::all_names() {
            let tt = TriggerType::parse(name).unwrap();
            assert_eq!(tt.as_str(), *name);
        }
    }

    #[test]
    fn test_trigger_type_invalid() {
        assert!(TriggerType::parse("bogus").is_none());
    }

    #[test]
    fn test_matches_message_empty_condition() {
        // Empty condition matches everything
        assert!(matches_message_event("{}", "agent-a", Some("ch"), "hello"));
    }

    #[test]
    fn test_matches_message_from_filter() {
        let cond = r#"{"from_agent":"agent-a"}"#;
        assert!(matches_message_event(cond, "agent-a", None, "msg"));
        assert!(!matches_message_event(cond, "agent-b", None, "msg"));
    }

    #[test]
    fn test_matches_message_channel_filter() {
        let cond = r#"{"channel":"alerts"}"#;
        assert!(matches_message_event(cond, "any", Some("alerts"), "msg"));
        assert!(!matches_message_event(cond, "any", Some("other"), "msg"));
        assert!(!matches_message_event(cond, "any", None, "msg"));
    }

    #[test]
    fn test_matches_message_payload_contains() {
        let cond = r#"{"payload_contains":"ERROR"}"#;
        assert!(matches_message_event(
            cond,
            "any",
            None,
            "got an ERROR here"
        ));
        assert!(!matches_message_event(cond, "any", None, "all good"));
    }

    #[test]
    fn test_matches_message_combined() {
        let cond = r#"{"from_agent":"monitor","channel":"alerts","payload_contains":"critical"}"#;
        assert!(matches_message_event(
            cond,
            "monitor",
            Some("alerts"),
            "critical failure"
        ));
        assert!(!matches_message_event(
            cond,
            "other",
            Some("alerts"),
            "critical failure"
        ));
        assert!(!matches_message_event(
            cond,
            "monitor",
            Some("alerts"),
            "minor issue"
        ));
    }

    #[test]
    fn test_matches_message_invalid_json() {
        assert!(!matches_message_event("not json", "a", None, "b"));
    }

    #[test]
    fn test_validate_trigger_type() {
        assert!(validate_trigger_type("file_watch").is_ok());
        assert!(validate_trigger_type("message_received").is_ok());
        assert!(validate_trigger_type("webhook_received").is_ok());
        assert!(validate_trigger_type("schedule_complete").is_ok());
        assert!(validate_trigger_type("on_message").is_ok());
        assert!(validate_trigger_type("on_session_start").is_ok());
        assert!(validate_trigger_type("on_session_end").is_ok());
        assert!(validate_trigger_type("on_schedule").is_ok());
        assert!(validate_trigger_type("nope").is_err());
    }

    #[test]
    fn test_validate_trigger_type_error_message_includes_valid_types() {
        let err = validate_trigger_type("invalid").unwrap_err();
        assert!(err.contains("file_watch"), "error should list valid types");
        assert!(
            err.contains("message_received"),
            "error should list valid types"
        );
        assert!(err.contains("invalid"), "error should mention the bad type");
    }

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_boundary() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // 3-byte UTF-8 char: should truncate at valid char boundary
        let text = "aaa\u{2603}bbb"; // snowman (3 bytes)
        let result = truncate(text, 4);
        assert_eq!(result, "aaa"); // can't fit the snowman in 4 bytes
    }

    #[test]
    fn test_trigger_type_all_names_complete() {
        let names = TriggerType::all_names();
        assert_eq!(names.len(), 8);
        // Every name should roundtrip
        for name in names {
            assert!(TriggerType::parse(name).is_some());
        }
    }

    #[test]
    fn test_message_condition_deserialize_default() {
        let cond: MessageCondition = serde_json::from_str("{}").unwrap();
        assert!(cond.from_agent.is_none());
        assert!(cond.channel.is_none());
        assert!(cond.payload_contains.is_none());
    }

    #[test]
    fn test_message_condition_serialize_skips_none() {
        let cond = MessageCondition {
            from_agent: Some("agent-a".into()),
            channel: None,
            payload_contains: None,
        };
        let json = serde_json::to_string(&cond).unwrap();
        assert!(json.contains("from_agent"));
        assert!(!json.contains("channel"));
        assert!(!json.contains("payload_contains"));
    }

    #[test]
    fn test_file_watch_condition_roundtrip() {
        let cond = FileWatchCondition {
            pattern: Some("src/**/*.rs".into()),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: FileWatchCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pattern, Some("src/**/*.rs".into()));
    }

    #[test]
    fn test_webhook_condition_roundtrip() {
        let cond = WebhookCondition {
            path: Some("/github/pr".into()),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: WebhookCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, Some("/github/pr".into()));
    }

    #[test]
    fn test_schedule_complete_condition_roundtrip() {
        let cond = ScheduleCompleteCondition {
            schedule_name: Some("nightly-build".into()),
        };
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: ScheduleCompleteCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schedule_name, Some("nightly-build".into()));
    }

    #[test]
    fn test_trigger_type_serde_roundtrip() {
        for tt in [
            TriggerType::FileWatch,
            TriggerType::MessageReceived,
            TriggerType::ScheduleComplete,
            TriggerType::WebhookReceived,
            TriggerType::OnMessage,
            TriggerType::OnSessionStart,
            TriggerType::OnSessionEnd,
            TriggerType::OnSchedule,
        ] {
            let json = serde_json::to_string(&tt).unwrap();
            let parsed: TriggerType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, tt);
        }
    }

    #[test]
    fn test_matches_on_message_empty_condition() {
        assert!(matches_on_message_event("{}", "alice", "hello world"));
    }

    #[test]
    fn test_matches_on_message_from_author_filter() {
        let cond = r#"{"from_author":"alice"}"#;
        assert!(matches_on_message_event(cond, "alice", "msg"));
        assert!(!matches_on_message_event(cond, "bob", "msg"));
    }

    #[test]
    fn test_matches_on_message_content_contains_filter() {
        let cond = r#"{"content_contains":"alert"}"#;
        assert!(matches_on_message_event(cond, "any", "critical alert!"));
        assert!(!matches_on_message_event(cond, "any", "all good"));
    }

    #[test]
    fn test_matches_on_message_combined() {
        let cond = r#"{"from_author":"monitor","content_contains":"error"}"#;
        assert!(matches_on_message_event(cond, "monitor", "error detected"));
        assert!(!matches_on_message_event(cond, "other", "error detected"));
        assert!(!matches_on_message_event(cond, "monitor", "all clear"));
    }

    #[test]
    fn test_matches_on_message_invalid_json() {
        assert!(!matches_on_message_event("not json", "a", "b"));
    }

    #[test]
    fn test_matches_on_session_empty_condition() {
        assert!(matches_on_session_event("{}", "discord"));
        assert!(matches_on_session_event("{}", "system"));
    }

    #[test]
    fn test_matches_on_session_platform_filter() {
        let cond = r#"{"platform":"discord"}"#;
        assert!(matches_on_session_event(cond, "discord"));
        assert!(!matches_on_session_event(cond, "slack"));
    }

    #[test]
    fn test_matches_on_session_invalid_json() {
        assert!(!matches_on_session_event("not json", "discord"));
    }

    #[test]
    fn test_matches_on_schedule_empty_condition() {
        assert!(matches_on_schedule_event("{}", "any-team"));
    }

    #[test]
    fn test_matches_on_schedule_team_filter() {
        let cond = r#"{"team":"code-review"}"#;
        assert!(matches_on_schedule_event(cond, "code-review"));
        assert!(!matches_on_schedule_event(cond, "bug-triage"));
    }

    #[test]
    fn test_matches_on_schedule_invalid_json() {
        assert!(!matches_on_schedule_event("not json", "team"));
    }
}
