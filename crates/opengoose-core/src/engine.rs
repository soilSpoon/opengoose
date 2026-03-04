use std::sync::Arc;

use dashmap::DashMap;
use tracing::{info, warn};

use opengoose_persistence::SessionStore;
use opengoose_profiles::ProfileStore;
use opengoose_teams::{HistoryEntry, TeamOrchestrator, TeamStore};
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
    /// SQLite-backed session and conversation history store.
    session_store: Arc<SessionStore>,
}

impl Engine {
    pub fn new(event_bus: EventBus, session_store: SessionStore) -> Self {
        let session_store = Arc::new(session_store);

        // Restore active teams from database
        let active_teams = DashMap::new();
        match session_store.load_all_active_teams() {
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
            session_store,
        }
    }

    // ── Team management ──────────────────────────────────────────────

    /// Set the active team for a session.
    pub fn set_active_team(&self, session_key: &SessionKey, team_name: String) {
        info!(%session_key, team = %team_name, "activating team");
        self.active_teams
            .insert(session_key.clone(), team_name.clone());
        if let Err(e) = self
            .session_store
            .set_active_team(session_key, Some(&team_name))
        {
            warn!(%e, "failed to persist active team");
        }
        self.event_bus.emit(AppEventKind::TeamActivated {
            session_key: session_key.clone(),
            team_name,
        });
    }

    /// Clear the active team for a session.
    pub fn clear_active_team(&self, session_key: &SessionKey) {
        info!(%session_key, "deactivating team");
        self.active_teams.remove(session_key);
        if let Err(e) = self.session_store.set_active_team(session_key, None) {
            warn!(%e, "failed to persist team deactivation");
        }
        self.event_bus.emit(AppEventKind::TeamDeactivated {
            session_key: session_key.clone(),
        });
    }

    /// Get the active team for a session.
    pub fn active_team_for(&self, session_key: &SessionKey) -> Option<String> {
        self.active_teams.get(session_key).map(|v| v.clone())
    }

    /// Check if a team exists in the team store.
    pub fn team_exists(&self, name: &str) -> bool {
        TeamStore::new()
            .ok()
            .and_then(|store| store.get(name).ok())
            .is_some()
    }

    /// List all available team names.
    pub fn list_teams(&self) -> Vec<String> {
        TeamStore::new()
            .ok()
            .and_then(|store| store.list().ok())
            .unwrap_or_default()
    }

    // ── Session / history management ─────────────────────────────────

    /// Record a user message in the conversation history.
    pub fn record_user_message(
        &self,
        key: &SessionKey,
        content: &str,
        author: Option<&str>,
    ) {
        if let Err(e) = self.session_store.append_user_message(key, content, author) {
            warn!(%e, "failed to persist user message");
        }
    }

    /// Record an assistant message in the conversation history.
    pub fn record_assistant_message(&self, key: &SessionKey, content: &str) {
        if let Err(e) = self.session_store.append_assistant_message(key, content) {
            warn!(%e, "failed to persist assistant message");
        }
    }

    /// Get a reference to the session store (for cleanup, etc).
    pub fn session_store(&self) -> &SessionStore {
        &self.session_store
    }

    /// Get a reference to the event bus.
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    // ── Message processing ───────────────────────────────────────────

    /// Process an incoming message. If a team is active for the session,
    /// runs team orchestration and returns `Some(response)`.
    /// If no team is active, returns `None` (caller should route to single-agent).
    pub async fn process_message(
        &self,
        session_key: &SessionKey,
        author: Option<&str>,
        text: &str,
    ) -> anyhow::Result<Option<String>> {
        // Emit event
        self.event_bus.emit(AppEventKind::MessageReceived {
            session_key: session_key.clone(),
            author: author.unwrap_or("unknown").to_string(),
            content: text.to_string(),
        });

        // Persist user message
        self.record_user_message(session_key, text, author);

        // Check if a team is active
        let team_name = match self.active_team_for(session_key) {
            Some(name) => name,
            None => return Ok(None),
        };

        // Run team orchestration
        let response = self
            .run_team_orchestration(session_key, &team_name, text)
            .await?;

        Ok(Some(response))
    }

    /// Execute a team workflow and return the result.
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

        let orchestrator = TeamOrchestrator::new(team, profile_store);

        // Load conversation history for context
        let history = self
            .session_store
            .load_history(session_key, 20)
            .unwrap_or_default();

        let history_entries: Vec<HistoryEntry> = history
            .iter()
            .map(|m| HistoryEntry {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();

        let response = orchestrator
            .execute_with_history(input, &history_entries)
            .await?;

        // Persist assistant response
        self.record_assistant_message(session_key, &response);

        self.event_bus.emit(AppEventKind::ResponseSent {
            session_key: session_key.clone(),
            content: response.clone(),
        });

        Ok(response)
    }
}
