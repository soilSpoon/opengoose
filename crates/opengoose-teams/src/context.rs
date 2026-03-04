use std::sync::Arc;

use opengoose_persistence::{
    Database, MessageQueue, MessageType, OrchestrationStore, SessionStore, WorkItemStore,
};
use opengoose_types::SessionKey;

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
}

impl OrchestrationContext {
    pub fn new(team_run_id: String, session_key: SessionKey, db: Arc<Database>) -> Self {
        Self {
            team_run_id,
            session_key,
            db,
        }
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
        let _ = self.queue().enqueue(
            &self.session_key.to_stable_id(),
            &self.team_run_id,
            sender,
            "broadcast",
            content,
            MessageType::Broadcast,
        );
    }

    /// Convenience: read all broadcasts for this run since a given message ID.
    pub fn read_broadcasts(
        &self,
        since_id: Option<i64>,
    ) -> Vec<opengoose_persistence::QueueMessage> {
        self.queue()
            .read_broadcasts(&self.team_run_id, since_id)
            .unwrap_or_default()
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }
}
