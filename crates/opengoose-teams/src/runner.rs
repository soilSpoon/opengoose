use std::sync::Arc;

use anyhow::{anyhow, Result};
use futures::StreamExt;
use tracing::{debug, info};
use uuid::Uuid;

use goose::agents::extension::ExtensionConfig;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use goose::providers::create_with_named_model;

use opengoose_profiles::AgentProfile;

/// A single conversation turn (user or assistant) for injecting history context.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub role: String,
    pub content: String,
}

/// Wraps a Goose `Agent` for one-shot execution from an `AgentProfile`.
pub struct AgentRunner {
    agent: Arc<Agent>,
    session_id: String,
    profile_name: String,
}

impl AgentRunner {
    /// Create a Goose Agent configured from an `AgentProfile`.
    pub async fn from_profile(profile: &AgentProfile) -> Result<Self> {
        let agent = Arc::new(Agent::new());
        let session_id = Uuid::new_v4().to_string();

        // Set provider/model
        let settings = profile.settings.as_ref();
        let provider_name = settings
            .and_then(|s| s.goose_provider.as_deref())
            .unwrap_or("anthropic");
        let model_name = settings
            .and_then(|s| s.goose_model.as_deref())
            .unwrap_or("claude-sonnet-4-20250514");

        let provider = create_with_named_model(provider_name, model_name, vec![])
            .await
            .map_err(|e| anyhow!("failed to create provider: {e}"))?;

        agent.update_provider(provider, &session_id).await?;

        // Set system prompt from instructions
        if let Some(instructions) = &profile.instructions {
            agent.override_system_prompt(instructions.clone()).await;
        } else if let Some(prompt) = &profile.prompt {
            agent.override_system_prompt(prompt.clone()).await;
        }

        // Add extensions
        for ext in &profile.extensions {
            let config = ExtensionConfig::Builtin {
                name: ext.name.clone(),
                description: String::new(),
                display_name: None,
                timeout: None,
                bundled: Some(true),
                available_tools: vec![],
            };
            if let Err(e) = agent.add_extension(config, &session_id).await {
                debug!(
                    ext = %ext.name,
                    error = %e,
                    "failed to add extension (non-fatal)"
                );
            }
        }

        info!(
            profile = %profile.name(),
            session_id = %session_id,
            "created agent runner"
        );

        Ok(Self {
            agent,
            session_id,
            profile_name: profile.title.clone(),
        })
    }

    /// The profile name this runner was created from.
    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    /// Send a message with conversation history and collect the full assistant response text.
    pub async fn run_with_history(
        &self,
        input: &str,
        history: &[HistoryEntry],
    ) -> Result<String> {
        // Build a context prefix from history if available
        let effective_input = if history.is_empty() {
            input.to_string()
        } else {
            let history_text: String = history
                .iter()
                .map(|h| format!("[{}]: {}", h.role, h.content))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "Conversation history:\n---\n{history_text}\n---\n\nCurrent message: {input}"
            )
        };

        self.run(&effective_input).await
    }

    /// Send a message and collect the full assistant response text.
    pub async fn run(&self, input: &str) -> Result<String> {
        let user_message = Message::user().with_text(input);

        let max_turns = 10; // reasonable default for team tasks

        let session_config = SessionConfig {
            id: self.session_id.clone(),
            schedule_id: None,
            max_turns: Some(max_turns),
            retry_config: None,
        };

        let mut stream = self.agent.reply(user_message, session_config, None).await?;

        let mut response_parts = Vec::new();
        while let Some(event_result) = stream.next().await {
            match event_result? {
                AgentEvent::Message(msg) => {
                    let text = msg.as_concat_text();
                    if !text.is_empty() {
                        response_parts.push(text);
                    }
                }
                _ => {} // ignore model changes, compaction events, etc.
            }
        }

        // The last assistant message typically contains the final response
        let response = response_parts
            .last()
            .cloned()
            .unwrap_or_default();

        debug!(
            profile = %self.profile_name,
            response_len = response.len(),
            "agent run complete"
        );

        Ok(response)
    }
}
