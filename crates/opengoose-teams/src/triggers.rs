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
            fire_matching_triggers(db, event_bus, "on_message", |cond, _| {
                matches_on_message_event(cond, author, content)
            })
            .await?;
        }
        AppEventKind::GooseReady => {
            fire_matching_triggers(db, event_bus, "on_session_start", |cond, _| {
                matches_on_session_event(cond, "system")
            })
            .await?;
        }
        AppEventKind::ChannelReady { platform } => {
            fire_matching_triggers(db, event_bus, "on_session_start", |cond, _| {
                matches_on_session_event(cond, &platform.to_string())
            })
            .await?;
        }
        AppEventKind::SessionDisconnected { session_key, .. } => {
            let platform = session_key.platform.to_string();
            fire_matching_triggers(db, event_bus, "on_session_end", |cond, _| {
                matches_on_session_event(cond, &platform)
            })
            .await?;
        }
        AppEventKind::TeamRunCompleted { team } => {
            fire_matching_triggers(db, event_bus, "on_schedule", |cond, trigger_team| {
                // Prevent self-triggering loops: skip if the trigger would fire
                // the same team that just completed.
                if trigger_team == team {
                    return false;
                }
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
    F: Fn(&str, &str) -> bool,
{
    let store = TriggerStore::new(db.clone());
    let triggers = store.list_by_type(trigger_type)?;

    for trigger in triggers {
        if matches(&trigger.condition_json, &trigger.team_name) {
            info!(
                trigger = %trigger.name,
                team = %trigger.team_name,
                trigger_type,
                "trigger matched: firing team run"
            );

            let input = if trigger.input.is_empty() {
                format!("Triggered by {} trigger '{}'", trigger_type, trigger.name)
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
        assert!(validate_trigger_type("nope").is_err());
    }
}
