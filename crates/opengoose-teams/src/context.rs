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
    ) -> crate::TeamResult<i32> {
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
    ) -> crate::TeamResult<i32> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_types::Platform;

    fn test_ctx() -> OrchestrationContext {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let bus = EventBus::new(16);
        let key = SessionKey::new(Platform::Discord, "g1", "ch1");
        let ctx = OrchestrationContext::new("run-1".into(), key, db, bus);
        // Ensure session exists for FK constraints
        ctx.sessions()
            .append_user_message(&ctx.session_key, "init", None)
            .unwrap();
        ctx
    }

    #[test]
    fn test_context_accessors() {
        let ctx = test_ctx();
        assert_eq!(ctx.team_run_id, "run-1");
        assert_eq!(ctx.session_key.channel_id, "ch1");
        // Verify store accessors don't panic
        let _ = ctx.sessions();
        let _ = ctx.queue();
        let _ = ctx.work_items();
        let _ = ctx.orchestration();
        let _ = ctx.db();
    }

    #[test]
    fn test_context_emit() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let key = SessionKey::new(Platform::Discord, "g1", "ch1");
        let ctx = OrchestrationContext::new("run-1".into(), key, db, bus);
        ctx.emit(AppEventKind::GooseReady);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let event = rx.recv().await.unwrap();
            assert!(matches!(event.kind, AppEventKind::GooseReady));
        });
    }

    #[test]
    fn test_create_work_item() {
        let ctx = test_ctx();
        let id = ctx.create_work_item("coder", None).unwrap();
        assert!(id > 0);

        let item = ctx.work_items().get(id).unwrap().unwrap();
        assert!(item.title.contains("coder"));
    }

    #[test]
    fn test_enqueue_message() {
        let ctx = test_ctx();
        let id = ctx
            .enqueue_message("coder", "reviewer", "check this", MessageType::Delegation)
            .unwrap();
        assert!(id > 0);

        let msgs = ctx
            .queue()
            .dequeue_delegations(&ctx.team_run_id, 10)
            .unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "check this");
    }

    #[test]
    fn test_broadcast_and_read() {
        let ctx = test_ctx();
        ctx.broadcast("coder", "found a bug");
        ctx.broadcast("reviewer", "confirmed");

        let broadcasts = ctx.read_broadcasts(None);
        assert_eq!(broadcasts.len(), 2);
        assert_eq!(broadcasts[0].content, "found a bug");
        assert_eq!(broadcasts[1].content, "confirmed");
    }

    #[test]
    fn test_read_broadcasts_with_since_id() {
        let ctx = test_ctx();
        ctx.broadcast("coder", "msg1");
        ctx.broadcast("reviewer", "msg2");

        let all = ctx.read_broadcasts(None);
        assert_eq!(all.len(), 2);
        let first_id = all[0].id;

        let since = ctx.read_broadcasts(Some(first_id));
        assert_eq!(since.len(), 1);
        assert_eq!(since[0].content, "msg2");
    }
}
