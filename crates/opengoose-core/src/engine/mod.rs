//! Engine implementation: session lifecycle and AI response streaming.
//!
//! [`Engine`] is the central coordinator. It accepts incoming messages from
//! a [`GatewayBridge`], creates or resumes sessions, invokes the Goose AI
//! backend, and streams responses back through the bridge.
//!
//! Sub-modules:
//! - `streaming` — incremental token delivery and cancellation.
//! - `team` — multi-agent team-mode orchestration within a session.

mod streaming;
mod team;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, info_span, warn};

use opengoose_persistence::{Database, OrchestrationStore, SessionStore};
use opengoose_profiles::ProfileStore;
use opengoose_teams::{TeamOrchestrator, TeamStore};
use opengoose_types::{AppEventKind, EventBus, SessionKey};

use crate::session_manager::SessionManager;
use crate::shutdown::{ShutdownController, ShutdownDrainResult, ShutdownSnapshot};

/// Platform-agnostic core engine.
///
/// Routes messages to either team orchestration (when a team is active)
/// or falls through to the Goose single-agent handler.
pub struct Engine {
    event_bus: EventBus,
    db: Arc<Database>,
    session_store: SessionStore,
    session_manager: SessionManager,
    /// Long-lived ProfileStore shared across all requests.
    /// Clones are cheap (Arc-backed file cache) and all benefit from
    /// cache hits populated by any clone, eliminating repeated disk reads.
    profile_store: Option<ProfileStore>,
    shutdown: ShutdownController,
    /// Cached TeamOrchestrators keyed by `"{session_stable_id}::{team_name}"`.
    ///
    /// Persisting orchestrators across messages keeps the agent pool alive
    /// between turns, avoiding MCP extension restarts on every message.
    orchestrator_cache: Arc<Mutex<HashMap<String, Arc<TeamOrchestrator>>>>,
}

impl Engine {
    pub fn new(event_bus: EventBus, db: Database) -> Self {
        let team_store = match opengoose_teams::TeamStore::new() {
            Ok(s) => Some(s),
            Err(e) => {
                warn!(%e, "failed to initialize team store");
                None
            }
        };

        Self::build(event_bus, db, team_store)
    }

    fn build(event_bus: EventBus, db: Database, team_store: Option<TeamStore>) -> Self {
        let db = Arc::new(db);

        // Suspend any incomplete orchestration runs from previous crash
        let orch_store = OrchestrationStore::new(db.clone());
        if let Err(e) = orch_store.suspend_incomplete() {
            warn!(%e, "failed to suspend incomplete team runs on startup");
        }

        let session_store = SessionStore::new(db.clone());

        let session_manager = SessionManager::new(event_bus.clone(), db.clone(), team_store);

        let profile_store = match ProfileStore::new() {
            Ok(s) => Some(s),
            Err(e) => {
                warn!(%e, "failed to initialize profile store");
                None
            }
        };

        Self {
            event_bus,
            db,
            session_store,
            session_manager,
            profile_store,
            shutdown: ShutdownController::new(),
            orchestrator_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[doc(hidden)]
    pub fn new_with_team_store(
        event_bus: EventBus,
        db: Database,
        team_store: Option<TeamStore>,
    ) -> Self {
        Self::build(event_bus, db, team_store)
    }

    // ── Message persistence (inlined) ───────────────────────────────

    pub fn record_user_message(&self, key: &SessionKey, content: &str, author: Option<&str>) {
        if let Err(e) = self.session_store.append_user_message(key, content, author) {
            warn!(%e, "failed to persist user message");
        }
    }

    pub fn record_assistant_message(&self, key: &SessionKey, content: &str) {
        if let Err(e) = self.session_store.append_assistant_message(key, content) {
            warn!(%e, "failed to persist assistant message");
        }
    }

    fn send_response(&self, session_key: &SessionKey, msg: &str) {
        self.record_assistant_message(session_key, msg);
        self.event_bus.emit(AppEventKind::ResponseSent {
            session_key: session_key.clone(),
            content: msg.to_string(),
        });
    }

    // ── Accessors ───────────────────────────────────────────────────

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub fn sessions(&self) -> &SessionStore {
        &self.session_store
    }

    pub fn is_accepting_messages(&self) -> bool {
        self.shutdown.is_accepting_messages()
    }

    pub fn begin_shutdown(&self) -> ShutdownSnapshot {
        self.shutdown.begin_shutdown()
    }

    pub async fn wait_for_shutdown_drain(
        &self,
        timeout: std::time::Duration,
    ) -> ShutdownDrainResult {
        self.shutdown.wait_for_streams(timeout).await
    }

    // ── Lifecycle ────────────────────────────────────────────────────

    /// Gracefully shut down the engine.
    ///
    /// Clears the orchestrator cache, dropping all cached `TeamOrchestrator`
    /// instances so their agent pools can be cleaned up. Any in-flight
    /// orchestrations that hold an `Arc` clone will finish naturally but
    /// no new orchestrations will reuse the cached instances.
    pub async fn shutdown(&self) {
        let _span = info_span!("engine_shutdown").entered();
        self.shutdown.mark_stopped();
        let count = {
            let mut cache = self.orchestrator_cache.lock().await;
            let count = cache.len();
            cache.clear();
            count
        };
        if count > 0 {
            debug!(count, "cleared orchestrator cache during shutdown");
        }
    }
}
