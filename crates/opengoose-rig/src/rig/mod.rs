// Rig<M: WorkMode> — Strategy pattern: Operator/Worker share structure.
//
// Operator = Rig<ChatMode>: persistent session, direct conversation. No Board.
// Worker   = Rig<TaskMode>: per-task session, pull loop. Board claim -> execute -> submit.
//
// Shared: process() — Agent.reply() + stream consumption.
// Difference: WorkMode determines session management.

mod operator;
mod worker;

use crate::conversation_log;
use crate::pipeline::Middleware;
use crate::work_mode::{ChatMode, EvolveMode, TaskMode, WorkInput, WorkMode};
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent};
use goose::conversation::message::Message;
use opengoose_board::Board;
use opengoose_board::work_item::RigId;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Rig<M> = persistent agent identity + Strategy.
///
/// M=ChatMode -> Operator (chat), M=TaskMode -> Worker (task).
/// board is only used by Worker -- Operator is None.
pub struct Rig<M: WorkMode> {
    pub id: RigId,
    board: Option<Arc<Board>>,
    agent: Agent,
    mode: M,
    cancel: CancellationToken,
    middleware: Vec<Arc<dyn Middleware>>,
}

/// Operator: user-facing. Persistent session. Direct conversation without Board.
pub type Operator = Rig<ChatMode>;

/// Worker: Board worker. Pull loop. Per-task session.
pub type Worker = Rig<TaskMode>;

/// Evolver: stamp detection -> skill generation. Per-analysis session.
pub type Evolver = Rig<EvolveMode>;

// -- Shared (all WorkMode) --

impl<M: WorkMode> Rig<M> {
    pub fn new(
        id: RigId,
        board: Arc<Board>,
        agent: Agent,
        mode: M,
        middleware: Vec<Arc<dyn Middleware>>,
    ) -> Self {
        Self {
            id,
            board: Some(board),
            agent,
            mode,
            cancel: CancellationToken::new(),
            middleware,
        }
    }

    /// Shared message processing pipeline.
    /// WorkMode determines session ID, then runs Agent.reply().
    /// Returns Err on stream error so caller can decide whether to submit.
    pub async fn process(&self, input: WorkInput) -> anyhow::Result<()> {
        let session_config = self.mode.session_config(&input);
        let session_id = session_config.id.clone();
        let message = Message::user().with_text(&input.text);

        conversation_log::append_entry(&session_id, "user", &input.text);

        let stream = self
            .agent
            .reply(message, session_config, Some(self.cancel.clone()))
            .await?;

        tokio::pin!(stream);
        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Message(msg)) => {
                    tracing::debug!(rig = %self.id, "agent message: {:?}", msg.role);
                    let role = format!("{:?}", msg.role);
                    let content = extract_text_content(&msg);
                    if !content.is_empty() {
                        conversation_log::append_entry(&session_id, &role, &content);
                    }
                }
                Err(e) => {
                    tracing::warn!(rig = %self.id, error = %e, "agent stream error");
                    conversation_log::append_entry(&session_id, "error", &e.to_string());
                    return Err(e);
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn agent(&self) -> &Agent {
        &self.agent
    }

    pub fn board(&self) -> Option<&Arc<Board>> {
        self.board.as_ref()
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }
}

/// Extract text content from a Message.
fn extract_text_content(msg: &Message) -> String {
    use goose::conversation::message::MessageContent;
    msg.content
        .iter()
        .filter_map(|c| match c {
            MessageContent::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_mode::{ChatMode, EvolveMode, TaskMode};

    #[test]
    fn extract_text_content_keeps_newline_between_segments() {
        let message = Message::user().with_text("A").with_text("B").with_text("C");
        assert_eq!(extract_text_content(&message), "A\nB\nC");
    }

    #[test]
    fn extract_text_content_empty_message_returns_empty_string() {
        let message = Message::user();
        assert_eq!(extract_text_content(&message), "");
    }

    #[test]
    fn extract_text_content_single_segment() {
        let message = Message::user().with_text("hello world");
        assert_eq!(extract_text_content(&message), "hello world");
    }

    #[tokio::test]
    async fn rig_new_board_getter_returns_some() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let agent = Agent::new();
        let rig: Rig<TaskMode> = Rig::new(RigId::new("test-rig"), board, agent, TaskMode, vec![]);
        assert!(rig.board().is_some());
    }

    #[tokio::test]
    async fn rig_new_id_is_stored() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let agent = Agent::new();
        let id = RigId::new("my-rig-id");
        let rig: Rig<TaskMode> = Rig::new(id.clone(), board, agent, TaskMode, vec![]);
        assert_eq!(rig.id, id);
    }

    #[tokio::test]
    async fn rig_cancel_token_starts_alive() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let agent = Agent::new();
        let rig: Rig<TaskMode> = Rig::new(RigId::new("alive-rig"), board, agent, TaskMode, vec![]);
        assert!(!rig.cancel_token().is_cancelled());
    }

    #[tokio::test]
    async fn rig_cancel_marks_token_cancelled() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let agent = Agent::new();
        let rig: Rig<TaskMode> = Rig::new(RigId::new("cancel-rig"), board, agent, TaskMode, vec![]);
        let token = rig.cancel_token();
        assert!(!token.is_cancelled());
        rig.cancel();
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn rig_agent_getter_does_not_panic() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let agent = Agent::new();
        let rig: Rig<TaskMode> =
            Rig::new(RigId::new("agent-getter"), board, agent, TaskMode, vec![]);
        let _ = rig.agent();
    }

    #[tokio::test]
    async fn operator_without_board_returns_none_for_board() {
        let agent = Agent::new();
        let op = Operator::without_board(RigId::new("op-1"), agent, "my-session");
        assert!(op.board().is_none());
    }

    #[tokio::test]
    async fn operator_without_board_cancel_token_starts_alive() {
        let agent = Agent::new();
        let op = Operator::without_board(RigId::new("op-alive"), agent, "sess");
        assert!(!op.cancel_token().is_cancelled());
    }

    #[tokio::test]
    async fn operator_without_board_cancel_works() {
        let agent = Agent::new();
        let op = Operator::without_board(RigId::new("op-cancel"), agent, "sess");
        let token = op.cancel_token();
        op.cancel();
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn worker_try_claim_on_empty_board_returns_false() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let agent = Agent::new();
        let worker = Worker::new(RigId::new("wkr-empty"), board, agent, TaskMode, vec![]);
        let repo_dir = std::env::current_dir().unwrap();
        let result = worker.try_claim_and_execute(&repo_dir).await.unwrap();
        assert!(!result, "empty board should return Ok(false)");
    }

    #[tokio::test]
    async fn worker_run_exits_when_pre_cancelled() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let agent = Agent::new();
        let worker = Worker::new(RigId::new("w-pre-cancel"), board, agent, TaskMode, vec![]);
        worker.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(5), worker.run())
            .await
            .expect("worker.run() should return quickly after cancel");
    }

    #[tokio::test]
    async fn evolver_new_sets_id_and_has_board() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let id = RigId::new("evlv-1");
        let agent = Agent::new();
        let evolver = Evolver::new(id.clone(), board, agent, EvolveMode, vec![]);
        assert_eq!(evolver.id, id);
        assert!(evolver.board().is_some());
    }

    #[tokio::test]
    async fn rig_chat_mode_board_getter_returns_some() {
        let board = Arc::new(Board::in_memory().await.unwrap());
        let agent = Agent::new();
        let rig: Rig<ChatMode> = Rig::new(
            RigId::new("chat-rig"),
            board,
            agent,
            ChatMode::new("sess"),
            vec![],
        );
        assert!(rig.board().is_some());
    }
}
