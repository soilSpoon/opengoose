use std::sync::Arc;

use dashmap::DashMap;
use tracing::{info, warn};
use uuid::Uuid;

use opengoose_persistence::{Database, OrchestrationStore, SessionStore};
use opengoose_profiles::ProfileStore;
use opengoose_teams::{OrchestrationContext, TeamOrchestrator, TeamStore};
use opengoose_types::{AppEventKind, EventBus, SessionKey};

/// Platform-agnostic core engine.
///
/// Owns session management, team management, and orchestration logic.
/// Adapters (Discord, Slack, CLI, Web) interact with this engine —
/// it knows nothing about any specific platform.
pub struct Engine {
    event_bus: EventBus,
    /// Per-session active team. Key = SessionKey, Value = team name.
    active_teams: DashMap<SessionKey, String>,
    /// Shared database for all persistence.
    db: Arc<Database>,
}

impl Engine {
    pub fn new(event_bus: EventBus, db: Database) -> Self {
        let db = Arc::new(db);
        let sessions = SessionStore::new(db.clone());

        // Suspend any incomplete orchestration runs from previous crash
        let orch_store = OrchestrationStore::new(db.clone());
        if let Err(e) = orch_store.suspend_incomplete() {
            warn!(%e, "failed to suspend incomplete runs on startup");
        }

        // Restore active teams from database
        let active_teams = DashMap::new();
        match sessions.load_all_active_teams() {
            Ok(teams) => {
                for (key, team) in teams {
                    info!(%key, team = %team, "restored active team from db");
                    active_teams.insert(key, team);
                }
            }
            Err(e) => {
                warn!(%e, "failed to restore active teams from db");
            }
        }

        Self {
            event_bus,
            active_teams,
            db,
        }
    }

    // ── Team management ──────────────────────────────────────────────

    pub fn set_active_team(&self, session_key: &SessionKey, team_name: String) {
        info!(%session_key, team = %team_name, "activating team");
        self.active_teams
            .insert(session_key.clone(), team_name.clone());
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.set_active_team(session_key, Some(&team_name)) {
            warn!(%e, "failed to persist active team");
        }
        self.event_bus.emit(AppEventKind::TeamActivated {
            session_key: session_key.clone(),
            team_name,
        });
    }

    pub fn clear_active_team(&self, session_key: &SessionKey) {
        info!(%session_key, "deactivating team");
        self.active_teams.remove(session_key);
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.set_active_team(session_key, None) {
            warn!(%e, "failed to persist team deactivation");
        }
        self.event_bus.emit(AppEventKind::TeamDeactivated {
            session_key: session_key.clone(),
        });
    }

    pub fn active_team_for(&self, session_key: &SessionKey) -> Option<String> {
        self.active_teams.get(session_key).map(|v| v.clone())
    }

    pub fn team_exists(&self, name: &str) -> bool {
        match TeamStore::new() {
            Ok(store) => match store.get(name) {
                Ok(_) => true,
                Err(_) => false,
            },
            Err(e) => {
                warn!("failed to open team store: {e}");
                false
            }
        }
    }

    pub fn list_teams(&self) -> Vec<String> {
        match TeamStore::new() {
            Ok(store) => match store.list() {
                Ok(teams) => teams,
                Err(e) => {
                    warn!("failed to list teams: {e}");
                    Default::default()
                }
            },
            Err(e) => {
                warn!("failed to open team store: {e}");
                Default::default()
            }
        }
    }

    // ── Session / history management ─────────────────────────────────

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

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub fn sessions(&self) -> SessionStore {
        SessionStore::new(self.db.clone())
    }

    // ── Message processing ───────────────────────────────────────────

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

        // Handle resume command
        if text.trim() == "!resume" {
            return self.handle_resume(session_key).await.map(Some);
        }

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
        let team_store =
            TeamStore::new().map_err(|e| anyhow::anyhow!("team store error: {e}"))?;
        let profile_store =
            ProfileStore::new().map_err(|e| anyhow::anyhow!("profile store error: {e}"))?;

        let team = team_store
            .get(team_name)
            .map_err(|e| anyhow::anyhow!("team load error: {e}"))?;

        let team_run_id = Uuid::new_v4().to_string();
        let ctx = OrchestrationContext::new(
            team_run_id,
            session_key.clone(),
            self.db.clone(),
        );

        let orchestrator = TeamOrchestrator::new(team, profile_store);
        let response = orchestrator.execute(input, &ctx).await?;

        self.record_assistant_message(session_key, &response);

        self.event_bus.emit(AppEventKind::ResponseSent {
            session_key: session_key.clone(),
            content: response.clone(),
        });

        Ok(response)
    }

    async fn handle_resume(&self, session_key: &SessionKey) -> anyhow::Result<String> {
        let orch_store = OrchestrationStore::new(self.db.clone());
        let suspended = orch_store.find_suspended(&session_key.to_stable_id())?;

        if suspended.is_empty() {
            return Ok("No suspended runs to resume.".to_string());
        }

        let run = &suspended[0];

        if run.workflow != "chain" {
            return Ok(format!(
                "Cannot resume: workflow type `{}` does not support resume. \
                 Only chain workflows can be resumed.",
                run.workflow
            ));
        }

        info!(
            team_run_id = %run.team_run_id,
            team = %run.team_name,
            step = run.current_step,
            "resuming suspended orchestration"
        );

        let team_store =
            TeamStore::new().map_err(|e| anyhow::anyhow!("team store error: {e}"))?;
        let profile_store =
            ProfileStore::new().map_err(|e| anyhow::anyhow!("profile store error: {e}"))?;

        let team = team_store
            .get(&run.team_name)
            .map_err(|e| anyhow::anyhow!("team load error: {e}"))?;

        let ctx = OrchestrationContext::new(
            run.team_run_id.clone(),
            session_key.clone(),
            self.db.clone(),
        );

        let work_items = ctx.work_items().list_for_run(&run.team_run_id, None)?;
        let parent_id = work_items
            .iter()
            .find(|wi| wi.parent_id.is_none())
            .map(|wi| wi.id.clone())
            .ok_or_else(|| anyhow::anyhow!("no parent work item found for run"))?;

        let orchestrator = TeamOrchestrator::new(team, profile_store);
        let response = orchestrator.resume(&ctx, &parent_id).await?;

        self.record_assistant_message(session_key, &response);
        self.event_bus.emit(AppEventKind::ResponseSent {
            session_key: session_key.clone(),
            content: response.clone(),
        });

        Ok(response)
    }
}
