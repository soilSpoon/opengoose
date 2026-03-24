// Operator-specific impl: user-facing chat without Board.

use super::Operator;
use crate::conversation_log;
use crate::work_mode::{ChatMode, WorkInput, WorkMode};
use goose::agents::{Agent, AgentEvent};
use goose::conversation::message::Message;
use opengoose_board::work_item::RigId;
use tokio_util::sync::CancellationToken;

impl Operator {
    /// Create without Board. Operator does not use a Board.
    pub fn without_board(id: RigId, agent: Agent, session_id: impl Into<String>) -> Self {
        Self {
            id,
            board: None,
            agent,
            mode: ChatMode::new(session_id),
            cancel: CancellationToken::new(),
            middleware: vec![],
        }
    }

    /// Direct conversation with user. Does not go through Board.
    /// Persistent session -> prompt cache guaranteed.
    pub async fn chat(&self, input: &str) -> anyhow::Result<()> {
        self.process(WorkInput::chat(input)).await
    }

    /// Streaming chat -- for TUI token-level display.
    /// Returns the Agent.reply() stream directly so the caller consumes events.
    pub async fn chat_streaming(
        &self,
        input: &str,
    ) -> anyhow::Result<impl futures::Stream<Item = Result<AgentEvent, anyhow::Error>>> {
        let session_config = self.mode.session_config(&WorkInput::chat(input));
        let session_id = session_config.id.clone();
        let message = Message::user().with_text(input);

        conversation_log::append_entry(&session_id, "user", input);

        self.agent
            .reply(message, session_config, Some(self.cancel.clone()))
            .await
    }
}
