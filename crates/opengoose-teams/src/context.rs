use std::sync::Arc;

use tracing::warn;

use opengoose_persistence::{
    Database, MessageQueue, MessageType, OrchestrationStore, SessionStore, WorkItemStore,
};
use opengoose_types::{AppEventKind, EventBus, SessionKey};

/// Shared context passed to all orchestration operations.
///
/// Provides access to session history, message queue, work items,
/// and orchestration run tracking through a single shared Database.
pub struct OrchestrationContext {
    /// Unique identifier for this orchestration run.
    pub team_run_id: String,
    /// Session this run belongs to.
    pub session_key: SessionKey,
    /// Shared database handle.
    db: Arc<Database>,
    /// Event bus for emitting orchestration events.
    event_bus: EventBus,
}

impl OrchestrationContext {
    pub fn new(
        team_run_id: String,
        session_key: SessionKey,
        db: Arc<Database>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            team_run_id,
            session_key,
            db,
            event_bus,
        }
    }

    /// Emit an event on the event bus.
    pub fn emit(&self, kind: AppEventKind) {
        self.event_bus.emit(kind);
    }

    pub fn sessions(&self) -> SessionStore {
        SessionStore::new(self.db.clone())
    }

    pub fn queue(&self) -> MessageQueue {
        MessageQueue::new(self.db.clone())
    }

    pub fn work_items(&self) -> WorkItemStore {
        WorkItemStore::new(self.db.clone())
    }

    pub fn orchestration(&self) -> OrchestrationStore {
        OrchestrationStore::new(self.db.clone())
    }

    /// Convenience: enqueue a broadcast message from an agent.
    pub fn broadcast(&self, sender: &str, content: &str) {
        if let Err(e) = self.queue().enqueue(
            &self.session_key.to_stable_id(),
            &self.team_run_id,
            sender,
            "broadcast",
            content,
            MessageType::Broadcast,
        ) {
            warn!("failed to enqueue broadcast from {sender}: {e}");
        }
    }

    /// Convenience: read all broadcasts for this run since a given message ID.
    pub fn read_broadcasts(
        &self,
        since_id: Option<i32>,
    ) -> Vec<opengoose_persistence::QueueMessage> {
        match self.queue().read_broadcasts(&self.team_run_id, since_id) {
            Ok(v) => v,
            Err(e) => {
                warn!("failed to read broadcasts for run {}: {e}", self.team_run_id);
                Default::default()
            }
        }
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }
}
