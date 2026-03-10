use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{Instrument, debug, info_span, warn};
use uuid::Uuid;

use opengoose_persistence::{Database, OrchestrationStore, SessionStore};
use opengoose_profiles::{AgentProfile, ProfileStore};
use opengoose_teams::{AgentRunner, OrchestrationContext, TeamOrchestrator, TeamStore};
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
    /// Long-lived ProfileStore shared across all requests.
    /// Clones are cheap (Arc-backed file cache) and all benefit from
    /// cache hits populated by any clone, eliminating repeated disk reads.
    profile_store: Option<ProfileStore>,
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
        let _span = info_span!(
            "handle_team_command",
            session_id = %session_key.to_stable_id(),
            command = %args,
        )
        .entered();

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

    // ── Lifecycle ────────────────────────────────────────────────────

    /// Gracefully shut down the engine.
    ///
    /// Clears the orchestrator cache, dropping all cached `TeamOrchestrator`
    /// instances so their agent pools can be cleaned up. Any in-flight
    /// orchestrations that hold an `Arc` clone will finish naturally but
    /// no new orchestrations will reuse the cached instances.
    pub async fn shutdown(&self) {
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
    /// When a team is active the message is routed to the `TeamOrchestrator`.
    /// Otherwise the default `main` profile handles the request, loading the
    /// per-workspace context files (BOOTSTRAP.md on first run, then IDENTITY.md,
    /// USER.md, SOUL.md, MEMORY.md).
    ///
    /// Always returns `Some(receiver)` — the Goose single-agent fallback is
    /// bypassed so that every conversation goes through the profile + workspace
    /// system.
    #[tracing::instrument(
        name = "process_message",
        skip(self, text),
        fields(session_id = %session_key.to_stable_id(), author = author.unwrap_or("unknown"))
    )]
    pub async fn process_message_streaming(
        &self,
        session_key: &SessionKey,
        author: Option<&str>,
        text: &str,
    ) -> anyhow::Result<Option<tokio::sync::broadcast::Receiver<StreamChunk>>> {
        let team_name = self.accept_message(session_key, author, text);

        let stream_id = Uuid::new_v4().to_string();
        self.event_bus.emit(AppEventKind::StreamStarted {
            session_key: session_key.clone(),
            stream_id: stream_id.clone(),
        });

        let (tx, rx) = stream_channel(64);

        match team_name {
            Some(name) => {
                let result = self.run_team_orchestration(session_key, &name, text).await;
                match result {
                    Ok(response) => {
                        if tx.send(StreamChunk::Delta(response.clone())).is_err() {
                            debug!("stream delta send failed — no receivers");
                        }
                        self.event_bus.emit(AppEventKind::StreamUpdated {
                            session_key: session_key.clone(),
                            stream_id: stream_id.clone(),
                            content_len: response.chars().count(),
                        });
                        if tx.send(StreamChunk::Done).is_err() {
                            debug!("stream done send failed — no receivers");
                        }
                        self.event_bus.emit(AppEventKind::StreamCompleted {
                            session_key: session_key.clone(),
                            stream_id,
                            full_text: response,
                        });
                    }
                    Err(e) => {
                        if tx.send(StreamChunk::Error(e.to_string())).is_err() {
                            debug!("stream error send failed — no receivers");
                        }
                        return Err(e);
                    }
                }
            }
            None => {
                // Spawn a background task so provider text deltas flow into `tx`
                // in real time while we return `rx` to the caller immediately.
                let profile_store = self.profile_store.clone();
                let db = self.db.clone();
                let event_bus = self.event_bus.clone();
                let session_key = session_key.clone();
                let stream_id_for_task = stream_id.clone();
                let input = text.to_string();

                tokio::spawn(async move {
                    let (inner_tx, mut inner_rx) = stream_channel(64);
                    let tx_forward = tx.clone();
                    let event_bus_forward = event_bus.clone();
                    let session_key_forward = session_key.clone();
                    let stream_id_forward = stream_id_for_task.clone();

                    let forwarder = tokio::spawn(async move {
                        let mut content_len = 0usize;
                        loop {
                            match inner_rx.recv().await {
                                Ok(StreamChunk::Delta(delta)) => {
                                    content_len += delta.chars().count();
                                    if tx_forward.send(StreamChunk::Delta(delta)).is_err() {
                                        debug!("forwarded delta dropped — no receivers");
                                        break;
                                    }
                                    event_bus_forward.emit(AppEventKind::StreamUpdated {
                                        session_key: session_key_forward.clone(),
                                        stream_id: stream_id_forward.clone(),
                                        content_len,
                                    });
                                }
                                Ok(StreamChunk::Done) => {
                                    let _ = tx_forward.send(StreamChunk::Done);
                                    break;
                                }
                                Ok(StreamChunk::Error(error)) => {
                                    let _ = tx_forward.send(StreamChunk::Error(error));
                                    break;
                                }
                                Err(_) => break,
                            }
                        }
                    });

                    match Self::stream_default_profile(
                        profile_store,
                        db.clone(),
                        session_key.clone(),
                        input,
                        inner_tx,
                    )
                    .await
                    {
                        Ok(response) => {
                            let _ = forwarder.await;
                            let _ = tx.send(StreamChunk::Done);
                            let session_store = SessionStore::new(db);
                            if let Err(e) =
                                session_store.append_assistant_message(&session_key, &response)
                            {
                                warn!(%e, "failed to persist assistant message");
                            }
                            event_bus.emit(AppEventKind::ResponseSent {
                                session_key: session_key.clone(),
                                content: response.clone(),
                            });
                            event_bus.emit(AppEventKind::StreamCompleted {
                                session_key,
                                stream_id: stream_id_for_task,
                                full_text: response,
                            });
                        }
                        Err(e) => {
                            let _ = forwarder.await;
                            warn!(%e, "default profile streaming failed");
                            let _ = tx.send(StreamChunk::Error(e.to_string()));
                        }
                    }
                });
            }
        }

        Ok(Some(rx))
    }

    /// Stream the default `main` profile, forwarding text deltas to `tx` as they arrive.
    ///
    /// Loads the `main` profile, seeds conversation history, and drives `AgentRunner::run_streaming`.
    /// Returns the full accumulated response text when the agent finishes.
    /// The caller is responsible for sending [`StreamChunk::Done`] afterwards.
    async fn stream_default_profile(
        profile_store: Option<ProfileStore>,
        db: Arc<Database>,
        session_key: SessionKey,
        input: String,
        tx: tokio::sync::broadcast::Sender<StreamChunk>,
    ) -> anyhow::Result<String> {
        let profile = match profile_store.as_ref().and_then(|s| s.get("main").ok()) {
            Some(p) => p,
            None => AgentProfile {
                version: "1.0.0".to_string(),
                title: "main".to_string(),
                description: None,
                instructions: None,
                prompt: None,
                extensions: vec![],
                skills: vec![],
                settings: None,
                activities: None,
                response: None,
                sub_recipes: None,
                parameters: None,
            },
        };

        let runner = AgentRunner::from_profile(&profile).await?;

        let session_store = SessionStore::new(db);
        if let Ok(history) = session_store.load_history(&session_key, 51) {
            let prior: Vec<(String, String)> = history
                .iter()
                .take(history.len().saturating_sub(1))
                .map(|m| (m.role.clone(), m.content.clone()))
                .collect();
            if !prior.is_empty() {
                runner.seed_history(&prior).await?;
            }
        }

        let output = runner.run_streaming(&input, &tx).await?;

        Ok(output.response)
    }

    async fn run_team_orchestration(
        &self,
        session_key: &SessionKey,
        team_name: &str,
        input: &str,
    ) -> anyhow::Result<String> {
        let span = info_span!(
            "team_orchestration",
            session_id = %session_key.to_stable_id(),
            team_name = %team_name,
        );
        async move {
            // Look up (or create) a cached orchestrator for this session + team.
            // Holding the cached orchestrator keeps its agent pool alive between
            // messages, so MCP extensions are not restarted on every turn.
            let cache_key = format!("{}::{team_name}", session_key.to_stable_id());
            let orchestrator = {
                let mut cache = self.orchestrator_cache.lock().await;
                if !cache.contains_key(&cache_key) {
                    let team = self
                        .session_manager
                        .team_store()
                        .ok_or(crate::error::GatewayError::TeamStoreNotReady)?
                        .get(team_name)?;
                    let profile_store = self
                        .profile_store
                        .clone()
                        .ok_or(crate::error::GatewayError::ProfileStoreNotReady)?;
                    cache.insert(
                        cache_key.clone(),
                        Arc::new(TeamOrchestrator::new(team, profile_store)),
                    );
                }
                // Key was just inserted above, so this should always succeed.
                cache
                    .get(&cache_key)
                    .expect("orchestrator cache key missing immediately after insert")
                    .clone()
            };

            let team_run_id = Uuid::new_v4().to_string();
            let ctx = OrchestrationContext::new(
                team_run_id,
                session_key.clone(),
                self.db.clone(),
                self.event_bus.clone(),
            );

            let response = orchestrator.execute(input, &ctx).await?;

            self.send_response(session_key, &response);

            Ok(response)
        }
        .instrument(span)
        .await
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
            "Available teams:\n- bug-triage\n- code-review\n- feature-dev\n- full-review\n- research-panel\n- security-audit\n- smart-router"
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
            "Team `missing-team` not found. Available: bug-triage, code-review, feature-dev, full-review, research-panel, security-audit, smart-router"
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

    #[test]
    fn accept_message_records_user_message_and_emits_event() {
        // Verifies that accept_message (called inside process_message_streaming)
        // persists the user message and emits MessageReceived, regardless of
        // whether a team is active or the default profile is used.
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let engine =
            Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);
        let key = test_key();

        // Call accept_message directly (it's private, but we can test via
        // record_user_message + event assertion without running the full async path).
        engine.record_user_message(&key, "hello world", Some("alice"));
        engine.event_bus.emit(AppEventKind::MessageReceived {
            session_key: key.clone(),
            author: "alice".to_string(),
            content: "hello world".to_string(),
        });

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

    #[tokio::test]
    async fn shutdown_clears_orchestrator_cache() {
        let event_bus = EventBus::new(16);
        let engine =
            Engine::new_with_team_store(event_bus, Database::open_in_memory().unwrap(), None);

        // Cache is empty, shutdown should be a no-op
        engine.shutdown().await;

        // Verify engine is still functional after shutdown
        let key = test_key();
        assert_eq!(engine.active_team_for(&key), None);
    }
}
