//! Workflow event trigger system.
//!
//! Two watchers are provided:
//!
//! - [`spawn_trigger_watcher`]: listens on the [`MessageBus`] for inter-agent
//!   messages and fires `message_received` triggers.
//! - [`spawn_event_bus_trigger_watcher`]: listens on the [`EventBus`] for
//!   system-level events (`on_message`, `on_session_start`, `on_session_end`,
//!   `on_schedule`) and fires the matching triggers.
mod evaluation;
mod handlers;

#[cfg(test)]
mod tests;

pub use evaluation::{
    FileWatchCondition, MessageCondition, OnMessageCondition, OnScheduleCondition,
    OnSessionCondition, ScheduleCompleteCondition, TriggerType, WebhookCondition,
    matches_file_watch_event, matches_message_event, matches_on_message_event,
    matches_on_schedule_event, matches_on_session_event, matches_webhook_path,
    validate_trigger_type,
};
pub use handlers::{
    spawn_event_bus_trigger_watcher, spawn_file_watch_trigger_watcher, spawn_trigger_watcher,
};
