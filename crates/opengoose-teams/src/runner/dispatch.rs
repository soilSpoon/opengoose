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
