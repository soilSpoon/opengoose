use std::sync::Arc;

use anyhow::{Result, anyhow};
use futures::StreamExt;
use tokio::sync::broadcast;
use tracing::{debug, info};
use uuid::Uuid;

use goose::agents::{Agent, AgentEvent, SessionConfig};

use goose::config::Config as GooseConfig;
use goose::conversation::message::Message;
use goose::providers::create_with_named_model;
use goose::session::SessionType;
use opengoose_types::StreamChunk;

use opengoose_profiles::AgentProfile;

use crate::recipe_bridge;

/// Last-resort provider/model if neither profile settings nor the system
/// Goose config (GOOSE_PROVIDER / GOOSE_MODEL) supply a value.
const FALLBACK_PROVIDER: &str = "anthropic";
const FALLBACK_MODEL: &str = "claude-sonnet-4-6";

#[derive(Debug, Clone)]
struct ProviderTarget {
    provider_name: String,
    model_name: String,
}

#[derive(Debug)]
struct AttemptFailure {
    error: anyhow::Error,
    emitted_content: bool,
}

impl AttemptFailure {
    fn new(error: anyhow::Error, emitted_content: bool) -> Self {
        Self {
            error,
            emitted_content,
        }
    }
}

fn resolve_provider_chain(profile: &AgentProfile) -> Vec<ProviderTarget> {
    let settings = profile.settings.as_ref();
    let goose_cfg = GooseConfig::global();
    let provider_name = settings
        .and_then(|s| s.goose_provider.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| {
            goose_cfg
                .get_param::<String>("GOOSE_PROVIDER")
                .unwrap_or_else(|_| FALLBACK_PROVIDER.to_string())
        });
    let model_name = settings
        .and_then(|s| s.goose_model.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| {
            goose_cfg
                .get_param::<String>("GOOSE_MODEL")
                .unwrap_or_else(|_| FALLBACK_MODEL.to_string())
        });

    let mut chain = vec![ProviderTarget {
        provider_name: provider_name.clone(),
        model_name: model_name.clone(),
    }];

    if let Some(settings) = settings {
        for fallback in &settings.provider_fallbacks {
            if fallback.goose_provider.trim().is_empty() {
                continue;
            }

            let target = ProviderTarget {
                provider_name: fallback.goose_provider.clone(),
                model_name: fallback
                    .goose_model
                    .clone()
                    .unwrap_or_else(|| model_name.clone()),
            };

            if chain.iter().any(|candidate| {
                candidate.provider_name == target.provider_name
                    && candidate.model_name == target.model_name
            }) {
                continue;
            }

            chain.push(target);
        }
    }

    chain
}

/// A parsed agent output: the main response plus any structured actions.
#[derive(Debug)]
pub struct AgentOutput {
    /// The final response text (with @mentions and [BROADCAST] lines stripped).
    pub response: String,
    /// Delegations detected: (recipient_agent, message).
    pub delegations: Vec<(String, String)>,
    /// Broadcast messages detected.
    pub broadcasts: Vec<String>,
}

/// Wraps a Goose `Agent` for one-shot execution from an `AgentProfile`.
pub struct AgentRunner {
    agent: Arc<Agent>,
    session_id: String,
    profile_name: String,
    provider_chain: Vec<ProviderTarget>,
    max_turns: u32,
    retry_config: Option<goose::agents::RetryConfig>,
}

impl AgentRunner {
    /// Create a Goose Agent configured from an `AgentProfile`.
    ///
    /// Generates a fresh random session ID on each call. Use
    /// `from_profile_keyed` when you want a stable, reusable session.
    pub async fn from_profile(profile: &AgentProfile) -> Result<Self> {
        Self::from_profile_keyed(profile, Uuid::new_v4().to_string()).await
    }

    /// Create a Goose Agent with an explicit session ID.
    ///
    /// When `session_id` is derived deterministically from user + agent context
    /// (e.g. `"{session_key}::{agent_name}"`), Goose reuses the same underlying
    /// session across invocations — preserving message history and (if
    /// `save_extension_state` was called previously) extension connections.
    pub async fn from_profile_keyed(profile: &AgentProfile, session_name: String) -> Result<Self> {
        let agent = Arc::new(Agent::new());

        // Create a real session in Goose's DB first. All subsequent Goose
        // calls (update_provider, add_extension, add_message, reply) require
        // the session row to exist due to FK constraints.
        let session = agent
            .config
            .session_manager
            .create_session(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")),
                session_name,
                SessionType::Gateway,
            )
            .await
            .map_err(|e| anyhow!("failed to create goose session: {e}"))?;
        let session_id = session.id;
        let settings = profile.settings.as_ref();
        let provider_chain = resolve_provider_chain(profile);

        // Set system prompt.
        //
        // If the profile carries explicit instructions (inline/team agents),
        // use them directly.  Otherwise build a workspace-backed identity:
        // seed the workspace on first run, load context files, and inject them
        // as an additive extension so the agent can read/modify them at runtime.
        if let Some(instructions) = &profile.instructions {
            agent.override_system_prompt(instructions.clone()).await;
        } else if let Some(prompt) = &profile.prompt {
            agent.override_system_prompt(prompt.clone()).await;
        } else {
            use opengoose_profiles::workspace;

            if let Some(workspace_dir) = workspace::workspace_dir_for(&profile.title) {
                if let Err(e) = workspace::setup_workspace(&profile.title, &workspace_dir) {
                    tracing::warn!(%e, profile = %profile.title, "failed to set up workspace");
                }

                let base_identity = format!(
                    "You are {}, a helpful AI assistant.\n\
                    Your workspace directory is: {}\n\
                    You have access to personal context files in your workspace. \
                    Read and follow any instructions contained in those files.",
                    profile.title,
                    workspace_dir.display()
                );
                agent.override_system_prompt(base_identity).await;

                let context = workspace::load_workspace_context(&workspace_dir);
                if !context.is_empty() {
                    agent
                        .extend_system_prompt("workspace_context".to_string(), context)
                        .await;
                }
            } else {
                let base_identity = format!("You are {}, a helpful AI assistant.", profile.title);
                agent.override_system_prompt(base_identity).await;
            }
        }

        // Add extensions — reuse the shared conversion from recipe_bridge.
        for ext in &profile.extensions {
            let config = match recipe_bridge::ext_ref_to_config(ext) {
                Some(c) => c,
                None => {
                    debug!(
                        ext = %ext.name,
                        ext_type = %ext.ext_type,
                        "skipping extension (unsupported type or missing required fields)"
                    );
                    continue;
                }
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

        let max_turns = settings.and_then(|s| s.max_turns).unwrap_or(10);
        let retry_config = settings.and_then(recipe_bridge::settings_to_retry_config);

        Ok(Self {
            agent,
            session_id,
            profile_name: profile.title.clone(),
            provider_chain,
            max_turns,
            retry_config,
        })
    }

    /// Create an agent runner from an inline system prompt (no profile file needed).
    pub async fn from_inline_prompt(system_prompt: &str, agent_name: &str) -> Result<Self> {
        let profile = AgentProfile {
            version: "1.0.0".to_string(),
            title: agent_name.to_string(),
            description: None,
            instructions: Some(system_prompt.to_string()),
            prompt: None,
            extensions: vec![],
            skills: vec![],
            settings: None,
            activities: None,
            response: None,
            sub_recipes: None,
            parameters: None,
        };
        Self::from_profile(&profile).await
    }

    /// Convenience: create from inline prompt and run in one call.
    pub async fn run_with_inline_prompt(
        system_prompt: &str,
        agent_name: &str,
        user_prompt: &str,
    ) -> Result<AgentOutput> {
        let runner = Self::from_inline_prompt(system_prompt, agent_name).await?;
        runner.run(user_prompt).await
    }

    /// The profile name this runner was created from.
    pub fn profile_name(&self) -> &str {
        &self.profile_name
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
    /// This uses Goose's native session management instead of baking
    /// conversation history into the prompt text, which:
    /// - Preserves message role structure (user vs. assistant)
    /// - Lets the provider handle context natively
    /// - Avoids redundant "Conversation history:" wrapper text
    pub async fn seed_history(&self, messages: &[(String, String)]) -> Result<()> {
        let session_mgr = &self.agent.config.session_manager;
        for (role, content) in messages {
            let msg = match role.as_str() {
                "user" => Message::user().with_text(content),
                "assistant" => Message::assistant().with_text(content),
                _ => Message::user().with_text(content),
            };
            session_mgr.add_message(&self.session_id, &msg).await?;
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

    async fn activate_provider(&self, target: &ProviderTarget) -> Result<()> {
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

/// Parse an agent's raw response text for @mentions and [BROADCAST] tags.
///
/// - `@agent_name: message` → delegation to another agent
/// - `[BROADCAST]: message` → broadcast to shared context log
///
/// Returns cleaned response text with these lines stripped.
pub fn parse_agent_output(raw: &str) -> AgentOutput {
    let mut response_lines = Vec::new();
    let mut delegations = Vec::new();
    let mut broadcasts = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();

        // Detect [BROADCAST]: ...
        if let Some(rest) = trimmed.strip_prefix("[BROADCAST]:") {
            broadcasts.push(rest.trim().to_string());
            continue;
        }

        // Detect @agent_name: message or @agent_name message
        if trimmed.starts_with('@')
            && let Some((agent, msg)) = parse_mention(trimmed)
        {
            delegations.push((agent, msg));
            continue;
        }

        response_lines.push(line);
    }

    let response = response_lines.join("\n").trim().to_string();

    AgentOutput {
        response,
        delegations,
        broadcasts,
    }
}

/// Parse an @mention line. Returns (agent_name, message) or None.
fn parse_mention(line: &str) -> Option<(String, String)> {
    let without_at = line.strip_prefix('@')?;

    // Try `@agent_name: message` first
    if let Some((agent, msg)) = without_at.split_once(':') {
        let agent = agent.trim();
        let msg = msg.trim();
        if !agent.is_empty() && !agent.contains(' ') && !msg.is_empty() {
            return Some((agent.to_string(), msg.to_string()));
        }
    }

    // Try `@agent_name message` (first word is agent, rest is message)
    let mut parts = without_at.splitn(2, ' ');
    let agent = parts.next()?.trim();
    let msg = parts.next()?.trim();
    if !agent.is_empty() && !msg.is_empty() {
        Some((agent.to_string(), msg.to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_broadcast() {
        let output = parse_agent_output(
            "Here's my analysis.\n[BROADCAST]: Found critical auth bug in line 42\nMore details here.",
        );
        assert_eq!(output.broadcasts.len(), 1);
        assert_eq!(output.broadcasts[0], "Found critical auth bug in line 42");
        assert_eq!(output.response, "Here's my analysis.\nMore details here.");
    }

    #[test]
    fn test_parse_mention_colon() {
        let output = parse_agent_output("@reviewer: please check the auth module");
        assert_eq!(output.delegations.len(), 1);
        assert_eq!(output.delegations[0].0, "reviewer");
        assert_eq!(output.delegations[0].1, "please check the auth module");
        assert!(output.response.is_empty());
    }

    #[test]
    fn test_parse_mention_space() {
        let output = parse_agent_output("@coder fix the bug in auth.rs");
        assert_eq!(output.delegations.len(), 1);
        assert_eq!(output.delegations[0].0, "coder");
        assert_eq!(output.delegations[0].1, "fix the bug in auth.rs");
    }

    #[test]
    fn test_mixed_output() {
        let raw = "Starting analysis.\n\
                   [BROADCAST]: database schema looks outdated\n\
                   @coder: update the migration files\n\
                   Here's the summary.\n\
                   [BROADCAST]: tests are all passing";
        let output = parse_agent_output(raw);
        assert_eq!(output.broadcasts.len(), 2);
        assert_eq!(output.delegations.len(), 1);
        assert_eq!(output.response, "Starting analysis.\nHere's the summary.");
    }

    #[test]
    fn test_no_special_output() {
        let output = parse_agent_output("Just a normal response with no special tags.");
        assert!(output.broadcasts.is_empty());
        assert!(output.delegations.is_empty());
        assert_eq!(
            output.response,
            "Just a normal response with no special tags."
        );
    }

    #[test]
    fn test_parse_mention_at_only() {
        // "@" alone should not be parsed as a mention
        let output = parse_agent_output("@");
        assert!(output.delegations.is_empty());
        assert_eq!(output.response, "@");
    }

    #[test]
    fn test_parse_mention_at_with_spaces() {
        // "@agent name with spaces: msg" — agent name has spaces, should not match colon form
        let output = parse_agent_output("@agent name with spaces: some message");
        // Falls through to space-based parsing: agent="agent", msg="name with spaces: some message"
        assert_eq!(output.delegations.len(), 1);
        assert_eq!(output.delegations[0].0, "agent");
    }

    #[test]
    fn test_parse_mention_no_message() {
        // "@coder" alone (no message) should not be a delegation
        let output = parse_agent_output("@coder");
        assert!(output.delegations.is_empty());
        assert_eq!(output.response, "@coder");
    }

    #[test]
    fn test_parse_mention_colon_empty_message() {
        // "@coder: " (empty after colon) — should not be parsed as delegation
        let output = parse_agent_output("@coder:");
        // colon form: agent="coder", msg="" → msg is empty → falls through to space form
        // space form: no space → returns None
        assert!(output.delegations.is_empty());
    }

    #[test]
    fn test_parse_broadcast_whitespace() {
        let output = parse_agent_output("[BROADCAST]:    extra spaces   ");
        assert_eq!(output.broadcasts.len(), 1);
        assert_eq!(output.broadcasts[0], "extra spaces");
    }

    #[test]
    fn test_parse_empty_input() {
        let output = parse_agent_output("");
        assert!(output.broadcasts.is_empty());
        assert!(output.delegations.is_empty());
        assert_eq!(output.response, "");
    }

    #[test]
    fn test_parse_only_whitespace_lines() {
        let output = parse_agent_output("  \n  \n  ");
        assert!(output.broadcasts.is_empty());
        assert!(output.delegations.is_empty());
    }

    #[test]
    fn test_multiple_delegations() {
        let raw = "@coder: fix the bug\n@reviewer: check the fix\n@tester run the tests";
        let output = parse_agent_output(raw);
        assert_eq!(output.delegations.len(), 3);
        assert_eq!(
            output.delegations[0],
            ("coder".into(), "fix the bug".into())
        );
        assert_eq!(
            output.delegations[1],
            ("reviewer".into(), "check the fix".into())
        );
        assert_eq!(
            output.delegations[2],
            ("tester".into(), "run the tests".into())
        );
        assert!(output.response.is_empty());
    }

    #[test]
    fn test_agent_output_profile_name() {
        // Verify AgentOutput fields are Debug-printable
        let output = AgentOutput {
            response: "hello".into(),
            delegations: vec![("a".into(), "b".into())],
            broadcasts: vec!["msg".into()],
        };
        let debug = format!("{:?}", output);
        assert!(debug.contains("hello"));
        assert!(debug.contains("msg"));
    }
}
