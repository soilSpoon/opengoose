use anyhow::Result;

use goose::conversation::message::Message;

use super::AgentRunner;

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
}
