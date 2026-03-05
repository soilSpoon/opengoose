use std::sync::Arc;

use tracing::warn;
use uuid::Uuid;

use opengoose_persistence::{Database, OrchestrationStore, SessionStore};
use opengoose_profiles::ProfileStore;
use opengoose_teams::{OrchestrationContext, TeamOrchestrator};
use opengoose_types::{AppEventKind, EventBus, SessionKey};

use crate::session_manager::SessionManager;

/// Platform-agnostic core engine.
///
/// Routes messages to either team orchestration (when a team is active)
/// or falls through to the Goose single-agent handler.
pub struct Engine {
    event_bus: EventBus,
    db: Arc<Database>,
    session_manager: SessionManager,
}

impl Engine {
    pub fn new(event_bus: EventBus, db: Database) -> Self {
        let db = Arc::new(db);

        // Suspend any incomplete orchestration runs from previous crash
        let orch_store = OrchestrationStore::new(db.clone());
        if let Err(e) = orch_store.suspend_incomplete() {
            warn!(%e, "failed to suspend incomplete team runs on startup");
        }

        let team_store = match opengoose_teams::TeamStore::new() {
            Ok(s) => Some(s),
            Err(e) => {
                warn!(%e, "failed to initialize team store");
                None
            }
        };

        let session_manager = SessionManager::new(event_bus.clone(), db.clone(), team_store);

        Self {
            event_bus,
            db,
            session_manager,
        }
    }

    // ── Team management ─────────────────────────────────────────────

    pub fn set_active_team(&self, session_key: &SessionKey, team_name: String) {
        self.session_manager.set_active_team(session_key, team_name);
    }

    pub fn clear_active_team(&self, session_key: &SessionKey) {
        self.session_manager.clear_active_team(session_key);
    }

    pub fn active_team_for(&self, session_key: &SessionKey) -> Option<String> {
        self.session_manager.active_team_for(session_key)
    }

    pub fn team_exists(&self, name: &str) -> bool {
        self.session_manager.team_exists(name)
    }

    pub fn list_teams(&self) -> Vec<String> {
        self.session_manager.list_teams()
    }

    // ── Message persistence (inlined) ───────────────────────────────

    pub fn record_user_message(&self, key: &SessionKey, content: &str, author: Option<&str>) {
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.append_user_message(key, content, author) {
            warn!(%e, "failed to persist user message");
        }
    }

    pub fn record_assistant_message(&self, key: &SessionKey, content: &str) {
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.append_assistant_message(key, content) {
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

    pub fn sessions(&self) -> SessionStore {
        SessionStore::new(self.db.clone())
    }

    // ── Message processing ──────────────────────────────────────────

    /// Process an incoming message. Returns Some(response) if handled by
    /// team orchestration, None if no team is active (fall through to Goose).
    pub async fn process_message(
        &self,
        session_key: &SessionKey,
        author: Option<&str>,
        text: &str,
    ) -> anyhow::Result<Option<String>> {
        self.event_bus.emit(AppEventKind::MessageReceived {
            session_key: session_key.clone(),
            author: author.unwrap_or("unknown").to_string(),
            content: text.to_string(),
        });

        self.record_user_message(session_key, text, author);

        let team_name = match self.active_team_for(session_key) {
            Some(name) => name,
            None => return Ok(None),
        };

        let response = self
            .run_team_orchestration(session_key, &team_name, text)
            .await?;

        Ok(Some(response))
    }

    async fn run_team_orchestration(
        &self,
        session_key: &SessionKey,
        team_name: &str,
        input: &str,
    ) -> anyhow::Result<String> {
        let team = self
            .session_manager
            .team_store()
            .ok_or_else(|| anyhow::anyhow!("team store not available"))?
            .get(team_name)
            .map_err(|e| anyhow::anyhow!("team load error: {e}"))?;

        let profile_store =
            ProfileStore::new().map_err(|e| anyhow::anyhow!("profile store error: {e}"))?;

        let team_run_id = Uuid::new_v4().to_string();
        let ctx = OrchestrationContext::new(
            team_run_id,
            session_key.clone(),
            self.db.clone(),
            self.event_bus.clone(),
        );

        let orchestrator = TeamOrchestrator::new(team, profile_store);
        let response = orchestrator.execute(input, &ctx).await?;

        self.send_response(session_key, &response);

        Ok(response)
    }
}
