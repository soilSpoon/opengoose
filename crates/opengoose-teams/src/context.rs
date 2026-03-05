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
///
/// Store instances are created once and cached for the lifetime of the context.
pub struct OrchestrationContext {
    /// Unique identifier for this orchestration run.
    pub team_run_id: String,
    /// Session this run belongs to.
    pub session_key: SessionKey,
    /// Shared database handle.
    db: Arc<Database>,
    /// Event bus for emitting orchestration events.
    event_bus: EventBus,
    /// Cached store instances -- created once, reused on every access.
    sessions: SessionStore,
    queue: MessageQueue,
    work_items: WorkItemStore,
    orchestration: OrchestrationStore,
}

impl OrchestrationContext {
    pub fn new(
        team_run_id: String,
        session_key: SessionKey,
        db: Arc<Database>,
        event_bus: EventBus,
    ) -> Self {
        let sessions = SessionStore::new(db.clone());
        let queue = MessageQueue::new(db.clone());
        let work_items = WorkItemStore::new(db.clone());
        let orchestration = OrchestrationStore::new(db.clone());
        Self {
            team_run_id,
            session_key,
            db,
            event_bus,
            sessions,
            queue,
            work_items,
            orchestration,
        }
    }

    /// Emit an event on the event bus.
    pub fn emit(&self, kind: AppEventKind) {
        self.event_bus.emit(kind);
    }

    /// Access the cached session store.
    pub fn sessions(&self) -> &SessionStore {
        &self.sessions
    }

    /// Access the cached message queue.
    pub fn queue(&self) -> &MessageQueue {
        &self.queue
    }

    /// Access the cached work item store.
    pub fn work_items(&self) -> &WorkItemStore {
        &self.work_items
    }

    /// Access the cached orchestration store.
    pub fn orchestration(&self) -> &OrchestrationStore {
        &self.orchestration
    }

    // -- Domain-specific convenience methods --

    /// Create a work item and return its integer ID.
    pub fn create_work_item(
        &self,
        agent_label: &str,
        parent_id: Option<i32>,
    ) -> anyhow::Result<i32> {
        self.work_items
            .create(
                &self.session_key.to_stable_id(),
                &self.team_run_id,
                agent_label,
                parent_id,
            )
            .map_err(Into::into)
    }

    /// Enqueue a message on the message queue for this run. Returns the message ID.
    pub fn enqueue_message(
        &self,
        sender: &str,
        recipient: &str,
        content: &str,
        msg_type: MessageType,
    ) -> anyhow::Result<i32> {
        self.queue
            .enqueue(
                &self.session_key.to_stable_id(),
                &self.team_run_id,
                sender,
                recipient,
                content,
                msg_type,
            )
            .map_err(Into::into)
    }

    /// Convenience: enqueue a broadcast message from an agent.
    pub fn broadcast(&self, sender: &str, content: &str) {
        if let Err(e) = self.queue.enqueue(
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
        match self.queue.read_broadcasts(&self.team_run_id, since_id) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "failed to read broadcasts for run {}: {e}",
                    self.team_run_id
                );
                Default::default()
            }
        }
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }
}
