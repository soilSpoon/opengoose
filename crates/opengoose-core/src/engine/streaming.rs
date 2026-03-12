use std::sync::Arc;

use tracing::{debug, info, info_span, instrument, warn};
use uuid::Uuid;

use opengoose_persistence::{Database, SessionStore};
use opengoose_profiles::{AgentProfile, ProfileStore};
use opengoose_teams::{AgentRunner, OrchestrationContext};
use opengoose_types::{AppEventKind, EventBus, SessionKey, StreamChunk, stream_channel};

use super::Engine;

impl Engine {
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
        let _span = info_span!(
            "accept_message",
            session_id = %session_key.to_stable_id(),
            author = author.unwrap_or("unknown"),
        )
        .entered();

        self.event_bus.emit(AppEventKind::MessageReceived {
            session_key: session_key.clone(),
            author: author.unwrap_or("unknown").to_string(),
            content: text.to_string(),
        });

        self.record_user_message(session_key, text, author);

        self.session_manager.active_team_for(session_key)
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
    #[instrument(
        skip(self, author, text),
        fields(
            session_id = %session_key.to_stable_id(),
            has_author = author.is_some(),
            text_len = text.chars().count()
        )
    )]
    pub async fn process_message_streaming(
        &self,
        session_key: &SessionKey,
        author: Option<&str>,
        text: &str,
    ) -> anyhow::Result<Option<tokio::sync::broadcast::Receiver<StreamChunk>>> {
        info!(session_id = %session_key.to_stable_id(), "process_message_streaming");
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
                        publish_team_stream_success(
                            &tx,
                            &self.event_bus,
                            session_key,
                            &stream_id,
                            response,
                        );
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
                    let (inner_tx, inner_rx) = stream_channel(64);
                    let tx_forward = tx.clone();
                    let event_bus_forward = event_bus.clone();
                    let session_key_forward = session_key.clone();
                    let stream_id_forward = stream_id_for_task.clone();

                    let forwarder = tokio::spawn(forward_stream_chunks(
                        inner_rx,
                        tx_forward,
                        event_bus_forward,
                        session_key_forward,
                        stream_id_forward,
                    ));

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
                            finish_default_profile_stream(
                                db,
                                &tx,
                                &event_bus,
                                &session_key,
                                &stream_id_for_task,
                                response,
                            );
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
    #[instrument(
        skip(profile_store, db, input, tx),
        fields(
            session_id = %session_key.to_stable_id(),
            input_len = input.chars().count()
        )
    )]
    async fn stream_default_profile(
        profile_store: Option<ProfileStore>,
        db: Arc<Database>,
        session_key: SessionKey,
        input: String,
        tx: tokio::sync::broadcast::Sender<StreamChunk>,
    ) -> anyhow::Result<String> {
        info!(session_id = %session_key.to_stable_id(), profile = "main", "stream_default_profile");

        let session_store = SessionStore::new(db);
        let selected_model = session_store
            .get_selected_model(&session_key)
            .ok()
            .flatten();
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
        }
        .with_model_override(selected_model.as_deref());

        let runner = AgentRunner::from_profile(&profile).await?;

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

    #[instrument(
        skip(self, input),
        fields(
            session_id = %session_key.to_stable_id(),
            team_name = %team_name,
            input_len = input.chars().count()
        )
    )]
    async fn run_team_orchestration(
        &self,
        session_key: &SessionKey,
        team_name: &str,
        input: &str,
    ) -> anyhow::Result<String> {
        // Look up (or create) a cached orchestrator for this session + team.
        // Holding the cached orchestrator keeps its agent pool alive between
        // messages, so MCP extensions are not restarted on every turn.
        info!(session_id = %session_key.to_stable_id(), team_name = %team_name, "team_orchestration");
        let selected_model = self
            .session_store
            .get_selected_model(session_key)
            .ok()
            .flatten();
        let model_cache_key = selected_model.as_deref().unwrap_or("__default__");
        let cache_key = format!(
            "{}::{team_name}::{model_cache_key}",
            session_key.to_stable_id()
        );
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
                    Arc::new(opengoose_teams::TeamOrchestrator::new_with_model_override(
                        team,
                        profile_store,
                        selected_model.clone(),
                    )),
                );
            }
            // Key was just inserted above, so this should always succeed.
            cache
                .get(&cache_key)
                .ok_or_else(|| {
                    anyhow::anyhow!("orchestrator cache key missing immediately after insert")
                })?
                .clone()
        };

        let team_run_id = Uuid::new_v4().to_string();
        let ctx = OrchestrationContext::new(
            team_run_id.clone(),
            session_key.clone(),
            self.db.clone(),
            self.event_bus.clone(),
        );

        debug!(session_id = %session_key.to_stable_id(), team_name = %team_name, team_run_id = %team_run_id, "team_execute");

        let response = orchestrator.execute(input, &ctx).await?;

        self.send_response(session_key, &response);

        Ok(response)
    }
}

fn publish_team_stream_success(
    tx: &tokio::sync::broadcast::Sender<StreamChunk>,
    event_bus: &EventBus,
    session_key: &SessionKey,
    stream_id: &str,
    response: String,
) {
    if tx.send(StreamChunk::Delta(response.clone())).is_err() {
        debug!("stream delta send failed — no receivers");
    }
    event_bus.emit(AppEventKind::StreamUpdated {
        session_key: session_key.clone(),
        stream_id: stream_id.to_string(),
        content_len: response.chars().count(),
    });
    if tx.send(StreamChunk::Done).is_err() {
        debug!("stream done send failed — no receivers");
    }
    event_bus.emit(AppEventKind::StreamCompleted {
        session_key: session_key.clone(),
        stream_id: stream_id.to_string(),
        full_text: response,
    });
}

async fn forward_stream_chunks(
    mut inner_rx: tokio::sync::broadcast::Receiver<StreamChunk>,
    tx: tokio::sync::broadcast::Sender<StreamChunk>,
    event_bus: EventBus,
    session_key: SessionKey,
    stream_id: String,
) {
    let mut content_len = 0usize;
    loop {
        match inner_rx.recv().await {
            Ok(StreamChunk::Delta(delta)) => {
                content_len += delta.chars().count();
                if tx.send(StreamChunk::Delta(delta)).is_err() {
                    debug!("forwarded delta dropped — no receivers");
                    break;
                }
                event_bus.emit(AppEventKind::StreamUpdated {
                    session_key: session_key.clone(),
                    stream_id: stream_id.clone(),
                    content_len,
                });
            }
            Ok(StreamChunk::Done) => {
                let _ = tx.send(StreamChunk::Done);
                break;
            }
            Ok(StreamChunk::Error(error)) => {
                let _ = tx.send(StreamChunk::Error(error));
                break;
            }
            Err(_) => break,
        }
    }
}

fn finish_default_profile_stream(
    db: Arc<Database>,
    tx: &tokio::sync::broadcast::Sender<StreamChunk>,
    event_bus: &EventBus,
    session_key: &SessionKey,
    stream_id: &str,
    response: String,
) {
    let _ = tx.send(StreamChunk::Done);
    let session_store = SessionStore::new(db);
    if let Err(e) = session_store.append_assistant_message(session_key, &response) {
        warn!(%e, "failed to persist assistant message");
    }
    event_bus.emit(AppEventKind::ResponseSent {
        session_key: session_key.clone(),
        content: response.clone(),
    });
    event_bus.emit(AppEventKind::StreamCompleted {
        session_key: session_key.clone(),
        stream_id: stream_id.to_string(),
        full_text: response,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use opengoose_teams::TeamStore;
    use opengoose_types::Platform;

    fn test_key() -> SessionKey {
        SessionKey::new(Platform::Discord, "guild-1", "channel-1")
    }

    fn temp_team_store() -> TeamStore {
        let dir =
            std::env::temp_dir().join(format!("opengoose-streaming-team-store-{}", Uuid::new_v4()));
        let store = TeamStore::with_dir(dir);
        store.install_defaults(false).unwrap();
        store
    }

    #[test]
    fn accept_message_records_user_message_emits_event_and_returns_active_team() {
        let event_bus = EventBus::new(16);
        let engine = Engine::new_with_team_store(
            event_bus,
            Database::open_in_memory().unwrap(),
            Some(temp_team_store()),
        );
        let key = test_key();
        engine
            .session_manager
            .set_active_team(&key, "code-review".into());

        let mut rx = engine.event_bus.subscribe();
        let team_name = engine.accept_message(&key, Some("alice"), "hello world");

        assert_eq!(team_name.as_deref(), Some("code-review"));

        let history = engine.sessions().load_history(&key, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "hello world");
        assert_eq!(history[0].author.as_deref(), Some("alice"));

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
    async fn publish_team_stream_success_sends_terminal_chunks_and_events() {
        let event_bus = EventBus::new(16);
        let mut event_rx = event_bus.subscribe();
        let key = test_key();
        let (tx, mut rx) = stream_channel(8);

        publish_team_stream_success(&tx, &event_bus, &key, "stream-1", "review complete".into());

        assert!(matches!(
            rx.recv().await.unwrap(),
            StreamChunk::Delta(delta) if delta == "review complete"
        ));
        assert!(matches!(rx.recv().await.unwrap(), StreamChunk::Done));
        assert!(matches!(
            event_rx.try_recv().unwrap().kind,
            AppEventKind::StreamUpdated {
                session_key,
                stream_id,
                content_len,
            } if session_key == key && stream_id == "stream-1" && content_len == "review complete".chars().count()
        ));
        assert!(matches!(
            event_rx.try_recv().unwrap().kind,
            AppEventKind::StreamCompleted {
                session_key,
                stream_id,
                full_text,
            } if session_key == key && stream_id == "stream-1" && full_text == "review complete"
        ));
    }

    #[tokio::test]
    async fn forward_stream_chunks_forwards_deltas_and_tracks_cumulative_length() {
        let event_bus = EventBus::new(16);
        let mut event_rx = event_bus.subscribe();
        let key = test_key();
        let (inner_tx, inner_rx) = stream_channel(8);
        let (outer_tx, mut outer_rx) = stream_channel(8);

        let forwarder = tokio::spawn(forward_stream_chunks(
            inner_rx,
            outer_tx,
            event_bus,
            key.clone(),
            "stream-2".into(),
        ));

        inner_tx.send(StreamChunk::Delta("hi".into())).unwrap();
        inner_tx.send(StreamChunk::Delta(" there".into())).unwrap();
        inner_tx.send(StreamChunk::Done).unwrap();
        forwarder.await.unwrap();

        assert!(matches!(
            outer_rx.recv().await.unwrap(),
            StreamChunk::Delta(delta) if delta == "hi"
        ));
        assert!(matches!(
            outer_rx.recv().await.unwrap(),
            StreamChunk::Delta(delta) if delta == " there"
        ));
        assert!(matches!(outer_rx.recv().await.unwrap(), StreamChunk::Done));
        assert!(matches!(
            event_rx.try_recv().unwrap().kind,
            AppEventKind::StreamUpdated {
                session_key,
                stream_id,
                content_len,
            } if session_key == key && stream_id == "stream-2" && content_len == 2
        ));
        assert!(matches!(
            event_rx.try_recv().unwrap().kind,
            AppEventKind::StreamUpdated {
                session_key,
                stream_id,
                content_len,
            } if session_key == key && stream_id == "stream-2" && content_len == 8
        ));
    }

    #[tokio::test]
    async fn forward_stream_chunks_forwards_errors_without_update_events() {
        let event_bus = EventBus::new(16);
        let mut event_rx = event_bus.subscribe();
        let key = test_key();
        let (inner_tx, inner_rx) = stream_channel(8);
        let (outer_tx, mut outer_rx) = stream_channel(8);

        let forwarder = tokio::spawn(forward_stream_chunks(
            inner_rx,
            outer_tx,
            event_bus,
            key,
            "stream-3".into(),
        ));

        inner_tx.send(StreamChunk::Error("boom".into())).unwrap();
        forwarder.await.unwrap();

        assert!(matches!(
            outer_rx.recv().await.unwrap(),
            StreamChunk::Error(error) if error == "boom"
        ));
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn finish_default_profile_stream_persists_response_and_emits_completion_events() {
        let event_bus = EventBus::new(16);
        let mut event_rx = event_bus.subscribe();
        let db = Arc::new(Database::open_in_memory().unwrap());
        let key = test_key();
        let (tx, mut rx) = stream_channel(8);

        finish_default_profile_stream(db.clone(), &tx, &event_bus, &key, "stream-4", "done".into());

        assert!(matches!(rx.try_recv().unwrap(), StreamChunk::Done));

        let history = SessionStore::new(db).load_history(&key, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "assistant");
        assert_eq!(history[0].content, "done");

        assert!(matches!(
            event_rx.try_recv().unwrap().kind,
            AppEventKind::ResponseSent {
                session_key,
                content,
            } if session_key == key && content == "done"
        ));
        assert!(matches!(
            event_rx.try_recv().unwrap().kind,
            AppEventKind::StreamCompleted {
                session_key,
                stream_id,
                full_text,
            } if session_key == key && stream_id == "stream-4" && full_text == "done"
        ));
    }
}
