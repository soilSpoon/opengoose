use std::sync::Arc;

use tracing::warn;
use uuid::Uuid;

use opengoose_persistence::{Database, OrchestrationStore, SessionStore};
use opengoose_profiles::ProfileStore;
use opengoose_teams::{OrchestrationContext, TeamOrchestrator, TeamStore};
use opengoose_types::{AppEventKind, EventBus, SessionKey, StreamChunk, stream_channel};

use crate::session_manager::SessionManager;

/// Platform-agnostic core engine.
///
/// Routes messages to either team orchestration (when a team is active)
/// or falls through to the Goose single-agent handler.
pub struct Engine {
    event_bus: EventBus,
    db: Arc<Database>,
    session_store: SessionStore,
    session_manager: SessionManager,
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

        Self {
            event_bus,
            db,
            session_store,
            session_manager,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_team_store(
        event_bus: EventBus,
        db: Database,
        team_store: Option<TeamStore>,
    ) -> Self {
        Self::build(event_bus, db, team_store)
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

    // ── Team command handling ─────────────────────────────────────────

    /// Handle a `/team` command and return the response text.
    ///
    /// Centralises team activation/deactivation/listing logic that was
    /// previously duplicated across every channel gateway.
    pub fn handle_team_command(&self, session_key: &SessionKey, args: &str) -> String {
        match args {
            "" => match self.active_team_for(session_key) {
                Some(team) => format!("Active team: {team}"),
                None => "No team active for this channel.".to_string(),
            },
            "off" => {
                self.clear_active_team(session_key);
                "Team deactivated. Reverting to single-agent mode.".to_string()
            }
            "list" => {
                let teams = self.list_teams();
                if teams.is_empty() {
                    "No teams available.".to_string()
                } else {
                    format!(
                        "Available teams:\n{}",
                        teams
                            .iter()
                            .map(|t| format!("- {t}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
            team_name => {
                if self.team_exists(team_name) {
                    self.set_active_team(session_key, team_name.to_string());
                    format!("Team {team_name} activated for this channel.")
                } else {
                    let available = self.list_teams();
                    format!(
                        "Team `{team_name}` not found. Available: {}",
                        if available.is_empty() {
                            "none".to_string()
                        } else {
                            available.join(", ")
                        }
                    )
                }
            }
        }
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

    // ── Message processing ──────────────────────────────────────────

    /// Record the incoming message and check for an active team.
    ///
    /// Returns `Some(team_name)` when a team should handle the message,
    /// `None` when the caller should fall through to the Goose single-agent.
    fn accept_message(
        &self,
        session_key: &SessionKey,
        author: Option<&str>,
        text: &str,
    ) -> Option<String> {
        self.event_bus.emit(AppEventKind::MessageReceived {
            session_key: session_key.clone(),
            author: author.unwrap_or("unknown").to_string(),
            content: text.to_string(),
        });

        self.record_user_message(session_key, text, author);

        self.active_team_for(session_key)
    }

    /// Process an incoming message with streaming support.
    ///
    /// Returns `Some(receiver)` if a team handles the message — the receiver
    /// will emit [`StreamChunk`] events as the response is generated.
    /// Returns `None` if no team is active (fall through to Goose single-agent).
    ///
    /// Currently the `TeamOrchestrator` returns a complete `String`, so the
    /// entire response is emitted as a single `Delta` followed by `Done`.
    /// When the LLM layer gains true token-level streaming, only this method
    /// needs to change — the downstream `drive_stream` infrastructure is ready.
    pub async fn process_message_streaming(
        &self,
        session_key: &SessionKey,
        author: Option<&str>,
        text: &str,
    ) -> anyhow::Result<Option<tokio::sync::broadcast::Receiver<StreamChunk>>> {
        let team_name = match self.accept_message(session_key, author, text) {
            Some(name) => name,
            None => return Ok(None),
        };

        let stream_id = Uuid::new_v4().to_string();
        self.event_bus.emit(AppEventKind::StreamStarted {
            session_key: session_key.clone(),
            stream_id: stream_id.clone(),
        });

        let (tx, rx) = stream_channel(64);

        // Run team orchestration (blocking until complete), then emit result
        let result = self
            .run_team_orchestration(session_key, &team_name, text)
            .await;

        match result {
            Ok(response) => {
                let _ = tx.send(StreamChunk::Delta(response.clone()));
                let _ = tx.send(StreamChunk::Done);
                self.event_bus.emit(AppEventKind::StreamCompleted {
                    session_key: session_key.clone(),
                    stream_id,
                    full_text: response,
                });
            }
            Err(ref e) => {
                let _ = tx.send(StreamChunk::Error(e.to_string()));
                return Err(result.unwrap_err());
            }
        }

        Ok(Some(rx))
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

#[cfg(test)]
mod tests {
    use super::*;

    use opengoose_types::Platform;

    fn test_key() -> SessionKey {
        SessionKey::new(Platform::Discord, "guild-1", "channel-1")
    }

    fn temp_team_store() -> TeamStore {
        let dir =
            std::env::temp_dir().join(format!("opengoose-engine-team-store-{}", Uuid::new_v4()));
        let store = TeamStore::with_dir(dir);
        store.install_defaults(false).unwrap();
        store
    }

    #[test]
    fn handle_team_command_activates_lists_and_clears_teams() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let engine = Engine::new_with_team_store(
            event_bus,
            Database::open_in_memory().unwrap(),
            Some(temp_team_store()),
        );
        let key = test_key();

        assert_eq!(
            engine.handle_team_command(&key, ""),
            "No team active for this channel."
        );
        assert_eq!(
            engine.handle_team_command(&key, "list"),
            "Available teams:\n- code-review\n- research-panel\n- smart-router"
        );
        assert_eq!(
            engine.handle_team_command(&key, "code-review"),
            "Team code-review activated for this channel."
        );
        assert_eq!(engine.active_team_for(&key), Some("code-review".into()));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::TeamActivated {
                session_key,
                team_name,
            } if session_key == key && team_name == "code-review"
        ));

        assert_eq!(
            engine.handle_team_command(&key, ""),
            "Active team: code-review"
        );
        assert_eq!(
            engine.handle_team_command(&key, "off"),
            "Team deactivated. Reverting to single-agent mode."
        );
        assert_eq!(engine.active_team_for(&key), None);
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::TeamDeactivated { session_key } if session_key == key
        ));
    }

    #[test]
    fn handle_team_command_reports_missing_team_choices() {
        let event_bus = EventBus::new(16);
        let engine = Engine::new_with_team_store(
            event_bus,
            Database::open_in_memory().unwrap(),
            Some(temp_team_store()),
        );
        let key = test_key();

        assert_eq!(
            engine.handle_team_command(&key, "missing-team"),
            "Team `missing-team` not found. Available: code-review, research-panel, smart-router"
        );
    }

    #[test]
    fn handle_team_command_without_store_uses_safe_defaults() {
        let event_bus = EventBus::new(16);
        let engine =
            Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
        let key = test_key();

        assert_eq!(
            engine.handle_team_command(&key, "list"),
            "No teams available."
        );
        assert_eq!(
            engine.handle_team_command(&key, "missing-team"),
            "Team `missing-team` not found. Available: none"
        );
    }

    #[test]
    fn records_messages_and_emits_responses() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let engine =
            Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
        let key = test_key();

        engine.record_user_message(&key, "hello", Some("alice"));
        engine.send_response(&key, "hi there");

        let history = engine.sessions().load_history(&key, 10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "hello");
        assert_eq!(history[0].author.as_deref(), Some("alice"));
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[1].content, "hi there");
        assert_eq!(history[1].author.as_deref(), Some("goose"));

        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::ResponseSent {
                session_key,
                content,
            } if session_key == key && content == "hi there"
        ));
    }

    #[tokio::test]
    async fn process_message_streaming_returns_none_without_active_team() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let engine =
            Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
        let key = test_key();

        let stream = engine
            .process_message_streaming(&key, Some("alice"), "hello world")
            .await
            .unwrap();

        assert!(stream.is_none());
        let history = engine.sessions().load_history(&key, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "hello world");
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::MessageReceived {
                session_key,
                author,
                content,
            } if session_key == key && author == "alice" && content == "hello world"
        ));
    }

    #[tokio::test]
    async fn process_message_streaming_errors_when_team_store_is_unavailable() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let engine =
            Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
        let key = test_key();
        engine
            .session_manager
            .set_active_team(&key, "code-review".into());

        let err = engine
            .process_message_streaming(&key, Some("alice"), "hello world")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("team store not available"));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::TeamActivated { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::MessageReceived { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::StreamStarted { session_key, .. } if session_key == key
        ));
    }
}
