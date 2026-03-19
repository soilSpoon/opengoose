// Rig<M: WorkMode> — Strategy 패턴으로 Operator/Worker 공유 구조.
//
// Operator = Rig<ChatMode>: 영속 세션, 직접 대화. Board를 거치지 않음.
// Worker   = Rig<TaskMode>: 작업당 세션, pull loop. Board에서 claim → execute → submit.
//
// 공유: process() — Agent.reply() 호출 + 스트림 소비.
// 차이: WorkMode가 세션 관리를 결정.

use crate::work_mode::{ChatMode, TaskMode, WorkInput, WorkMode};
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent};
use goose::conversation::message::Message;
use opengoose_board::work_item::RigId;
use opengoose_board::Board;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Rig<M> = 영속 에이전트 정체성 + Strategy.
///
/// M이 ChatMode이면 Operator (대화), TaskMode이면 Worker (작업).
pub struct Rig<M: WorkMode> {
    pub id: RigId,
    board: Arc<Mutex<Board>>,
    agent: Agent,
    mode: M,
    cancel: CancellationToken,
}

/// Operator: 사용자 대면. 영속 세션. Board를 거치지 않고 직접 대화.
pub type Operator = Rig<ChatMode>;

/// Worker: Board 워커. Pull loop. 작업당 세션.
pub type Worker = Rig<TaskMode>;

// ── 공유 (모든 WorkMode) ─────────────────────────────────────

impl<M: WorkMode> Rig<M> {
    pub fn new(id: RigId, board: Arc<Mutex<Board>>, agent: Agent, mode: M) -> Self {
        Self {
            id,
            board,
            agent,
            mode,
            cancel: CancellationToken::new(),
        }
    }

    /// 공유 메시지 처리 파이프라인.
    /// WorkMode가 세션 ID를 결정하고, Agent.reply()로 실행.
    /// 스트림 에러 발생 시 Err를 반환하여 호출자가 submit 여부를 판단.
    pub async fn process(&self, input: WorkInput) -> anyhow::Result<()> {
        let session_config = self.mode.session_config(&input);
        let message = Message::user().with_text(&input.text);

        let stream = self
            .agent
            .reply(message, session_config, Some(self.cancel.clone()))
            .await?;

        tokio::pin!(stream);
        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Message(msg)) => {
                    tracing::debug!(rig = %self.id, "agent message: {:?}", msg.role);
                }
                Err(e) => {
                    warn!(rig = %self.id, error = %e, "agent stream error");
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

    pub fn board(&self) -> &Arc<Mutex<Board>> {
        &self.board
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }
}

// ── Operator 전용 ────────────────────────────────────────────

impl Operator {
    /// 사용자와 직접 대화. Board를 통과하지 않음.
    /// 영속 세션 → prompt cache 보장.
    pub async fn chat(&self, input: &str) -> anyhow::Result<()> {
        self.process(WorkInput::chat(input)).await
    }
}

// ── Worker 전용 ──────────────────────────────────────────────

impl Worker {
    /// Pull loop. Board에서 작업을 기다리고, claim → execute → submit.
    /// Operator에는 이 메서드가 없음 — 컴파일타임 보장.
    pub async fn run(&self) {
        info!(rig = %self.id, "worker started, waiting for work");

        loop {
            let notify = {
                let board = self.board.lock().await;
                board.notify_handle()
            };

            tokio::select! {
                _ = notify.notified() => {
                    if let Err(e) = self.try_claim_and_execute().await {
                        warn!(rig = %self.id, error = %e, "execution failed");
                    }
                }
                _ = self.cancel.cancelled() => {
                    info!(rig = %self.id, "worker cancelled");
                    break;
                }
            }
        }
    }

    /// Board에서 가장 높은 우선순위 작업을 가져가서 실행.
    async fn try_claim_and_execute(&self) -> anyhow::Result<()> {
        let mut board = self.board.lock().await;
        let ready = board.ready();

        let Some(item) = ready.first() else {
            return Ok(());
        };

        let item = board.claim(item.id, &self.id)?;
        info!(rig = %self.id, item_id = item.id, title = %item.title, "claimed work item");
        drop(board);

        // Strategy가 세션 ID를 결정 (TaskMode: "task-{id}")
        let input = WorkInput::task(
            format!("Work item #{}: {}\n\n{}", item.id, item.title, item.description),
            item.id,
        );
        self.process(input).await?;

        // 완료 제출
        let mut board = self.board.lock().await;
        board.submit(item.id, &self.id)?;
        info!(rig = %self.id, item_id = item.id, "submitted work item");

        Ok(())
    }
}
