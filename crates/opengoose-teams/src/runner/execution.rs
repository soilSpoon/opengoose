use anyhow::{Result, anyhow};
use futures::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, info};

use goose::agents::{AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use goose::providers::create_with_named_model;
use opengoose_types::StreamChunk;

use super::AgentRunner;
use super::output::parse_agent_output;
use super::types::{AgentEventSummary, AgentOutput, AttemptFailure, ProviderTarget};

impl AgentRunner {
    /// The profile name this runner was created from.
    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    /// The working directory for this runner's Goose session.
    pub fn cwd(&self) -> &std::path::Path {
        &self.cwd
    }

    /// Add a keyed system prompt extension via Goose's `extend_system_prompt`.
    ///
    /// Unlike `override_system_prompt`, this is additive — it appends a named
    /// instruction block without replacing the base instructions. Useful for
    /// injecting team context (role, broadcast log) while preserving the
    /// original profile instructions.
    pub async fn extend_system_prompt(&self, key: &str, instruction: &str) {
        self.agent
            .extend_system_prompt(key.to_string(), instruction.to_string())
            .await;
    }

    /// The Goose session ID for this runner.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Save extension state to the Goose session for later restoration.
    ///
    /// Useful for persisting tool state across chain resume or session
    /// interruption. Call before the runner is dropped.
    pub async fn save_extension_state(&self) -> Result<()> {
        self.agent.persist_extension_state(&self.session_id).await
    }

    /// Restore extension state from the current Goose session.
    ///
    /// Call after creating a runner to restore tool connections and state
    /// from a prior session (e.g., during chain resume). Returns the number
    /// of extensions that failed to load.
    pub async fn load_extensions_from_session(&self) -> Result<usize> {
        let session = self
            .agent
            .config
            .session_manager
            .get_session(&self.session_id, false)
            .await?;
        let results = self.agent.load_extensions_from_session(&session).await;
        let failed = results.iter().filter(|r| !r.success).count();
        Ok(failed)
    }

    /// Seed the agent's Goose session with prior conversation messages.
    ///
    /// Applies Goose's `fix_conversation()` pipeline before seeding, which:
    /// - Removes orphaned tool requests/responses
    /// - Merges consecutive same-role messages
    /// - Ensures the conversation starts with a user message
    /// - Trims empty content
    ///
    /// This produces a cleaner context for the LLM provider.
    pub async fn seed_history(&self, messages: &[(String, String)]) -> Result<()> {
        use goose::conversation::{Conversation, fix_conversation};

        // Build Message objects from the role/content pairs.
        let goose_messages: Vec<Message> = messages
            .iter()
            .filter(|(_, content)| !content.trim().is_empty())
            .map(|(role, content)| match role.as_str() {
                "assistant" => Message::assistant().with_text(content),
                _ => Message::user().with_text(content),
            })
            .collect();

        if goose_messages.is_empty() {
            return Ok(());
        }

        // Apply Goose's conversation repair pipeline to clean up the history.
        let conversation = Conversation::new_unvalidated(goose_messages);
        let (fixed, _warnings) = fix_conversation(conversation);

        let session_mgr = &self.agent.config.session_manager;
        for msg in fixed.messages() {
            session_mgr.add_message(&self.session_id, msg).await?;
        }
        Ok(())
    }

    /// Register a JSON schema for structured output via Goose's FinalOutputTool.
    ///
    /// When set, the agent is instructed to call `recipe__final_output` with a
    /// JSON object matching the schema, ensuring validated structured output.
    pub async fn set_response_schema(&self, schema: serde_json::Value) {
        let response = goose::recipe::Response {
            json_schema: Some(schema),
        };
        self.agent.add_final_output_tool(response).await;
    }

    /// Run and return the raw text response (useful after `set_response_schema`).
    ///
    /// When a response schema is set, the last assistant message typically
    /// contains the validated JSON from the FinalOutputTool.
    pub async fn run_structured(&self, input: &str) -> Result<String> {
        for (index, target) in self.provider_chain.iter().enumerate() {
            if let Err(err) = self.activate_provider(target).await {
                if index + 1 < self.provider_chain.len() {
                    info!(
                        profile = %self.profile_name,
                        provider = %target.provider_name,
                        model = %target.model_name,
                        error = %err,
                        "provider activation failed, trying fallback"
                    );
                    continue;
                }
                return Err(err);
            }

            match self.run_structured_once(input).await {
                Ok(output) => return Ok(output),
                Err(failure)
                    if index + 1 < self.provider_chain.len() && !failure.emitted_content =>
                {
                    info!(
                        profile = %self.profile_name,
                        provider = %target.provider_name,
                        model = %target.model_name,
                        error = %failure.error,
                        "provider attempt failed before output, trying fallback"
                    );
                }
                Err(failure) => return Err(failure.error),
            }
        }

        Err(anyhow!(
            "no provider targets configured for {}",
            self.profile_name
        ))
    }

    async fn run_structured_once(
        &self,
        input: &str,
    ) -> std::result::Result<String, AttemptFailure> {
        let user_message = Message::user().with_text(input);

        let session_config = SessionConfig {
            id: self.session_id.clone(),
            schedule_id: None,
            max_turns: Some(self.max_turns),
            retry_config: self.retry_config.clone(),
        };

        let mut stream = self
            .agent
            .reply(user_message, session_config, None)
            .await
            .map_err(|err| AttemptFailure::new(err, false))?;

        let mut last_text = String::new();
        while let Some(event_result) = stream.next().await {
            let event =
                event_result.map_err(|err| AttemptFailure::new(err, !last_text.is_empty()))?;
            if let AgentEvent::Message(msg) = event {
                let text = msg.as_concat_text();
                if !text.is_empty() {
                    last_text = text;
                }
            }
        }

        Ok(last_text)
    }

    /// Send a message and collect the full response, parsing @mentions and broadcasts.
    pub async fn run(&self, input: &str) -> Result<AgentOutput> {
        for (index, target) in self.provider_chain.iter().enumerate() {
            if let Err(err) = self.activate_provider(target).await {
                if index + 1 < self.provider_chain.len() {
                    info!(
                        profile = %self.profile_name,
                        provider = %target.provider_name,
                        model = %target.model_name,
                        error = %err,
                        "provider activation failed, trying fallback"
                    );
                    continue;
                }
                return Err(err);
            }

            match self.run_once(input).await {
                Ok(output) => return Ok(output),
                Err(failure)
                    if index + 1 < self.provider_chain.len() && !failure.emitted_content =>
                {
                    info!(
                        profile = %self.profile_name,
                        provider = %target.provider_name,
                        model = %target.model_name,
                        error = %failure.error,
                        "provider attempt failed before output, trying fallback"
                    );
                }
                Err(failure) => return Err(failure.error),
            }
        }

        Err(anyhow!(
            "no provider targets configured for {}",
            self.profile_name
        ))
    }

    async fn run_once(&self, input: &str) -> std::result::Result<AgentOutput, AttemptFailure> {
        let user_message = Message::user().with_text(input);

        let session_config = SessionConfig {
            id: self.session_id.clone(),
            schedule_id: None,
            max_turns: Some(self.max_turns),
            retry_config: self.retry_config.clone(),
        };

        let mut stream = self
            .agent
            .reply(user_message, session_config, None)
            .await
            .map_err(|err| AttemptFailure::new(err, false))?;

        let mut response_parts = Vec::new();
        while let Some(event_result) = stream.next().await {
            let event =
                event_result.map_err(|err| AttemptFailure::new(err, !response_parts.is_empty()))?;
            if let AgentEvent::Message(msg) = event {
                let text = msg.as_concat_text();
                if !text.is_empty() {
                    response_parts.push(text);
                }
            }
        }

        let raw_response = response_parts.join("");

        debug!(
            profile = %self.profile_name,
            response_len = raw_response.len(),
            "agent run complete"
        );

        Ok(parse_agent_output(&raw_response))
    }

    /// Send a message and stream text deltas via `tx` as they arrive from the provider.
    ///
    /// Each non-empty text chunk emitted by Goose is forwarded immediately as a
    /// [`StreamChunk::Delta`]. The caller is responsible for sending
    /// [`StreamChunk::Done`] (or [`StreamChunk::Error`]) after this returns.
    pub async fn run_streaming(
        &self,
        input: &str,
        tx: &broadcast::Sender<StreamChunk>,
    ) -> Result<AgentOutput> {
        for (index, target) in self.provider_chain.iter().enumerate() {
            if let Err(err) = self.activate_provider(target).await {
                if index + 1 < self.provider_chain.len() {
                    info!(
                        profile = %self.profile_name,
                        provider = %target.provider_name,
                        model = %target.model_name,
                        error = %err,
                        "provider activation failed, trying fallback"
                    );
                    continue;
                }
                return Err(err);
            }

            match self.run_streaming_once(input, tx).await {
                Ok(output) => return Ok(output),
                Err(failure)
                    if index + 1 < self.provider_chain.len() && !failure.emitted_content =>
                {
                    info!(
                        profile = %self.profile_name,
                        provider = %target.provider_name,
                        model = %target.model_name,
                        error = %failure.error,
                        "provider attempt failed before output, trying fallback"
                    );
                }
                Err(failure) => return Err(failure.error),
            }
        }

        Err(anyhow!(
            "no provider targets configured for {}",
            self.profile_name
        ))
    }

    async fn run_streaming_once(
        &self,
        input: &str,
        tx: &broadcast::Sender<StreamChunk>,
    ) -> std::result::Result<AgentOutput, AttemptFailure> {
        let user_message = Message::user().with_text(input);

        let session_config = SessionConfig {
            id: self.session_id.clone(),
            schedule_id: None,
            max_turns: Some(self.max_turns),
            retry_config: self.retry_config.clone(),
        };

        let mut stream = self
            .agent
            .reply(user_message, session_config, None)
            .await
            .map_err(|err| AttemptFailure::new(err, false))?;

        let mut response_parts = Vec::new();
        while let Some(event_result) = stream.next().await {
            let event =
                event_result.map_err(|err| AttemptFailure::new(err, !response_parts.is_empty()))?;
            if let AgentEvent::Message(msg) = event {
                let text = msg.as_concat_text();
                if !text.is_empty() {
                    let _ = tx.send(StreamChunk::Delta(text.clone()));
                    response_parts.push(text);
                }
            }
        }

        let raw_response = response_parts.join("");

        debug!(
            profile = %self.profile_name,
            response_len = raw_response.len(),
            "agent run complete"
        );

        Ok(parse_agent_output(&raw_response))
    }

    /// Like `run()` but also collects non-message Goose events.
    ///
    /// Returns the parsed agent output together with a summary of model changes,
    /// context compactions, and MCP notifications observed during the run.
    pub async fn run_with_events(&self, input: &str) -> Result<(AgentOutput, AgentEventSummary)> {
        for (index, target) in self.provider_chain.iter().enumerate() {
            if let Err(err) = self.activate_provider(target).await {
                if index + 1 < self.provider_chain.len() {
                    info!(
                        profile = %self.profile_name,
                        provider = %target.provider_name,
                        model = %target.model_name,
                        error = %err,
                        "provider activation failed, trying fallback"
                    );
                    continue;
                }
                return Err(err);
            }

            match self.run_with_events_once(input).await {
                Ok(output) => return Ok(output),
                Err(failure)
                    if index + 1 < self.provider_chain.len() && !failure.emitted_content =>
                {
                    info!(
                        profile = %self.profile_name,
                        provider = %target.provider_name,
                        model = %target.model_name,
                        error = %failure.error,
                        "provider attempt failed before output, trying fallback"
                    );
                }
                Err(failure) => return Err(failure.error),
            }
        }

        Err(anyhow!(
            "no provider targets configured for {}",
            self.profile_name
        ))
    }

    async fn run_with_events_once(
        &self,
        input: &str,
    ) -> std::result::Result<(AgentOutput, AgentEventSummary), AttemptFailure> {
        let user_message = Message::user().with_text(input);

        let session_config = SessionConfig {
            id: self.session_id.clone(),
            schedule_id: None,
            max_turns: Some(self.max_turns),
            retry_config: self.retry_config.clone(),
        };

        let mut stream = self
            .agent
            .reply(user_message, session_config, None)
            .await
            .map_err(|err| AttemptFailure::new(err, false))?;

        let mut response_parts = Vec::new();
        let mut events = AgentEventSummary::default();

        while let Some(event_result) = stream.next().await {
            match event_result
                .map_err(|err| AttemptFailure::new(err, !response_parts.is_empty()))?
            {
                AgentEvent::Message(msg) => {
                    let text = msg.as_concat_text();
                    if !text.is_empty() {
                        response_parts.push(text);
                    }
                }
                AgentEvent::ModelChange { model, mode } => {
                    info!(
                        profile = %self.profile_name,
                        %model, %mode,
                        "model changed during agent run"
                    );
                    events.model_changes.push((model, mode));
                }
                AgentEvent::HistoryReplaced(_) => {
                    info!(
                        profile = %self.profile_name,
                        "context compacted during agent run"
                    );
                    events.context_compactions += 1;
                }
                AgentEvent::McpNotification((ext_name, _notification)) => {
                    debug!(
                        profile = %self.profile_name,
                        extension = %ext_name,
                        "MCP notification received"
                    );
                    events.extension_notifications.push(ext_name);
                }
            }
        }

        let raw_response = response_parts.join("");

        debug!(
            profile = %self.profile_name,
            response_len = raw_response.len(),
            model_changes = events.model_changes.len(),
            context_compactions = events.context_compactions,
            ext_notifications = events.extension_notifications.len(),
            "agent run with events complete"
        );

        Ok((parse_agent_output(&raw_response), events))
    }

    pub(super) async fn activate_provider(&self, target: &ProviderTarget) -> Result<()> {
        let provider = create_with_named_model(&target.provider_name, &target.model_name, vec![])
            .await
            .map_err(|e| {
                anyhow!(
                    "failed to create provider {} / {}: {e}",
                    target.provider_name,
                    target.model_name
                )
            })?;

        self.agent
            .update_provider(provider, &self.session_id)
            .await
            .map_err(|e| {
                anyhow!(
                    "failed to activate provider {} / {}: {e}",
                    target.provider_name,
                    target.model_name
                )
            })
    }
}
