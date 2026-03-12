use std::sync::Arc;

use tracing::{debug, info, info_span, instrument, warn};
use uuid::Uuid;

use opengoose_persistence::{Database, SessionStore};
use opengoose_profiles::{AgentProfile, ProfileStore};
use opengoose_teams::{AgentRunner, OrchestrationContext};
use opengoose_types::{AppEventKind, SessionKey, StreamChunk, stream_channel};

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
            if !self.orchestrator_cache.contains_key(&cache_key) {
                let team = self
                    .session_manager
                    .team_store()
                    .ok_or(crate::error::GatewayError::TeamStoreNotReady)?
                    .get(team_name)?;
                let profile_store = self
                    .profile_store
                    .clone()
                    .ok_or(crate::error::GatewayError::ProfileStoreNotReady)?;
                self.orchestrator_cache.insert(
                    cache_key.clone(),
                    Arc::new(opengoose_teams::TeamOrchestrator::new_with_model_override(
                        team,
                        profile_store,
                        selected_model.clone(),
                    )),
                );
            }
            // Key was just inserted above, so this should always succeed.
            self.orchestrator_cache
                .get(&cache_key)
                .ok_or_else(|| {
                    anyhow::anyhow!("orchestrator cache key missing immediately after insert")
                })?
                .value()
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
