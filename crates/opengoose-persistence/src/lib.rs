//! SQLite persistence layer for OpenGoose.
//!
//! All durable state lives here: sessions, agent messages, work items,
//! triggers, schedules, run status, alerts, event history, plugins, and the
//! message queue.
//! Built on Diesel with SQLite. The primary entry point is [`Database`],
//! which is cloned cheaply across threads (connection-pool backed).

mod agent_messages;
mod alerts;
mod api_key_store;
mod compact;
mod db;
mod db_enum;
mod error;
mod event_store;
mod memory_store;
mod message_queue;
mod models;
mod orchestration;
mod plugin_store;
mod prime;
pub mod prolly;
mod ready;
mod relationships;
mod run_status;
mod schedule_store;
mod schema;
mod session_store;
mod trigger_store;
mod work_items;

#[cfg(test)]
mod test_helpers;

pub use agent_messages::{AgentMessage, AgentMessageStatus, AgentMessageStore};
pub use alerts::{
    AlertAction, AlertCondition, AlertHistoryEntry, AlertHistoryQuery, AlertMetric, AlertRule,
    AlertStore, SystemMetrics,
};
pub use api_key_store::{ApiKeyInfo, ApiKeyStore, GeneratedApiKey};
pub use compact::CompactStore;
pub use db::Database;
pub use error::{PersistenceError, PersistenceResult};
pub use event_store::{
    DEFAULT_EVENT_RETENTION_DAYS, EventHistoryEntry, EventHistoryQuery, EventHistoryRecorderHandle,
    EventStore, normalize_since_filter, spawn_event_history_recorder,
};
pub use memory_store::{AgentMemory, MemoryStore};
pub use message_queue::{MessageQueue, MessageStatus, MessageType, QueueMessage, QueueStats};
pub use orchestration::{OrchestrationRun, OrchestrationStore};
pub use plugin_store::{Plugin, PluginStore};
pub use prime::PrimeStore;
pub use prolly::{ProllyBeadsStore, ProllyWorkItem, generate_hash_id};
pub use ready::{ReadyOptions, ReadyStore};
pub use relationships::{RelationStore, RelationType};
pub use run_status::RunStatus;
pub use schedule_store::{Schedule, ScheduleStore, ScheduleUpdate};
pub use session_store::{
    HistoryMessage, SessionItem, SessionStats, SessionStore, render_batch_session_exports_markdown,
    render_session_export_markdown,
};
pub use trigger_store::{Trigger, TriggerStore};
pub use work_items::{WorkItem, WorkItemStore, WorkStatus};
