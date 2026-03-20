// Rig<M: WorkMode> — Strategy 패턴으로 Operator/Worker 공유 구조.
//
// Operator = Rig<ChatMode>: 영속 세션, 직접 대화. Board를 거치지 않음.
// Worker   = Rig<TaskMode>: 작업당 세션, pull loop. Board에서 claim → execute → submit.
//
// 공유: process() — Agent.reply() 호출 + 스트림 소비.
// 차이: WorkMode가 세션 관리를 결정.

use crate::conversation_log;
use crate::work_mode::{ChatMode, EvolveMode, TaskMode, WorkInput, WorkMode};
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent};
use goose::conversation::message::Message;
use opengoose_board::work_item::{RigId, WorkItem};
use opengoose_board::Board;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Rig<M> = 영속 에이전트 정체성 + Strategy.
///
/// M이 ChatMode이면 Operator (대화), TaskMode이면 Worker (작업).
/// board는 Worker만 사용 — Operator는 None.
pub struct Rig<M: WorkMode> {
    pub id: RigId,
    board: Option<Arc<Board>>,
    agent: Agent,
    mode: M,
    cancel: CancellationToken,
}

/// Operator: 사용자 대면. 영속 세션. Board를 거치지 않고 직접 대화.
pub type Operator = Rig<ChatMode>;

/// Worker: Board 워커. Pull loop. 작업당 세션.
pub type Worker = Rig<TaskMode>;

/// Evolver: stamp 감지 → 스킬 생성. 분석당 세션.
pub type Evolver = Rig<EvolveMode>;

// ── 공유 (모든 WorkMode) ─────────────────────────────────────

impl<M: WorkMode> Rig<M> {
    pub fn new(id: RigId, board: Arc<Board>, agent: Agent, mode: M) -> Self {
        Self {
            id,
            board: Some(board),
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
        let session_id = session_config.id.clone();
        let message = Message::user().with_text(&input.text);

        // 사용자 입력 로깅
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
                    // 어시스턴트 메시지 로깅
                    let role = format!("{:?}", msg.role);
                    let content = extract_text_content(&msg);
                    if !content.is_empty() {
                        conversation_log::append_entry(&session_id, &role, &content);
                    }
                }
                Err(e) => {
                    warn!(rig = %self.id, error = %e, "agent stream error");
                    conversation_log::append_entry(
                        &session_id,
                        "error",
                        &e.to_string(),
                    );
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

// ── Operator 전용 ────────────────────────────────────────────

impl Operator {
    /// Board 없이 생성. Operator는 Board를 사용하지 않음.
    pub fn without_board(id: RigId, agent: Agent, session_id: impl Into<String>) -> Self {
        Self {
            id,
            board: None,
            agent,
            mode: ChatMode::new(session_id),
            cancel: CancellationToken::new(),
        }
    }

    /// 사용자와 직접 대화. Board를 통과하지 않음.
    /// 영속 세션 → prompt cache 보장.
    pub async fn chat(&self, input: &str) -> anyhow::Result<()> {
        self.process(WorkInput::chat(input)).await
    }

    /// 스트리밍 채팅 — TUI에서 토큰 단위 표시용.
    /// Agent.reply() 스트림을 직접 반환하여 호출자가 이벤트를 소비.
    pub async fn chat_streaming(
        &self,
        input: &str,
    ) -> anyhow::Result<impl futures::Stream<Item = Result<AgentEvent, anyhow::Error>>> {
        let session_config = self.mode.session_config(&WorkInput::chat(input));
        let session_id = session_config.id.clone();
        let message = Message::user().with_text(input);

        // 사용자 입력 로깅
        conversation_log::append_entry(&session_id, "user", input);

        self.agent
            .reply(message, session_config, Some(self.cancel.clone()))
            .await
    }
}

// ── Worker 전용 ──────────────────────────────────────────────

impl Worker {
    /// Pull loop. Board에서 작업을 기다리고, claim → execute → submit.
    /// Operator에는 이 메서드가 없음 — 컴파일타임 보장.
    /// board가 None이면 즉시 리턴 (Worker는 항상 board를 가짐).
    pub async fn run(&self) {
        let Some(board) = &self.board else {
            warn!(rig = %self.id, "worker has no board, exiting");
            return;
        };
        info!(rig = %self.id, "worker started, waiting for work");

        // Phase 1: Resume — 이전에 claim한 아이템 처리
        let stale = board.claimed_by(&self.id).await.unwrap_or_default();
        if !stale.is_empty() {
            info!(rig = %self.id, count = stale.len(), "resuming previously claimed items");
        }
        for item in &stale {
            if self.cancel.is_cancelled() { break; }
            self.process_claimed_item(item, board).await;
        }

        loop {
            // 1. 관심 먼저 등록 — 이 시점 이후의 모든 알림을 캡처
            let notify = board.notify_handle();
            let notified = notify.notified();

            // 2. 준비된 항목 확인 + 실행
            match self.try_claim_and_execute().await {
                Ok(true) => continue,  // 작업 발견, 즉시 추가 확인
                Ok(false) => {}        // 작업 없음, 대기로 이동
                Err(e) => warn!(rig = %self.id, error = %e, "execution failed"),
            }

            // 3. 작업 없음 — 알림 대기 (손실 불가능)
            tokio::select! {
                _ = notified => {}
                _ = self.cancel.cancelled() => {
                    info!(rig = %self.id, "worker cancelled");
                    break;
                }
            }
        }
    }

    /// Board에서 가장 높은 우선순위 작업을 가져가서 실행.
    async fn try_claim_and_execute(&self) -> anyhow::Result<bool> {
        let board = self.board.as_ref().expect("Worker must have a board");
        let ready = board.ready().await?;

        let Some(item) = ready.first() else {
            return Ok(false);
        };

        let item = board.claim(item.id, &self.id).await?;
        info!(rig = %self.id, item_id = item.id, title = %item.title, "claimed work item");

        self.process_claimed_item(&item, board).await;
        Ok(true)
    }

    /// claim된 아이템을 처리. 세션 조회/생성 → process → submit or abandon.
    /// 에러는 내부에서 처리하고 호출자에게 전파하지 않음.
    async fn process_claimed_item(&self, item: &WorkItem, board: &Arc<Board>) {
        let session_name = format!("task-{}", item.id);

        // 기존 세션 조회 → 없으면 새로 생성
        let (session_id, resuming) = match self.find_session_by_name(&session_name).await {
            Some(id) => {
                info!(rig = %self.id, item_id = item.id, "resuming existing session");
                (id, true)
            }
            None => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
                match self
                    .agent
                    .config
                    .session_manager
                    .create_session(
                        cwd,
                        session_name,
                        goose::session::session_manager::SessionType::User,
                    )
                    .await
                {
                    Ok(s) => (s.id, false),
                    Err(e) => {
                        warn!(rig = %self.id, item_id = item.id, error = %e, "failed to create session, abandoning");
                        board.abandon(item.id).await.ok();
                        return;
                    }
                }
            }
        };

        let prompt = if resuming {
            format!("Continue working on item #{}: {}", item.id, item.title)
        } else {
            format!("Work item #{}: {}\n\n{}", item.id, item.title, item.description)
        };

        let input = WorkInput::task(prompt, item.id).with_session_id(session_id);

        let result = self.process(input).await;
        match result {
            Ok(()) => {
                if let Err(e) = board.submit(item.id, &self.id).await {
                    warn!(rig = %self.id, item_id = item.id, error = %e, "submit failed");
                } else {
                    info!(rig = %self.id, item_id = item.id, "submitted work item");
                }
            }
            Err(e) => {
                warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
                if let Err(e) = board.abandon(item.id).await {
                    warn!(rig = %self.id, item_id = item.id, error = %e, "abandon failed");
                }
            }
        }
    }

    /// goose session_manager에서 name으로 세션 조회. 마지막(최신) 매칭 반환.
    async fn find_session_by_name(&self, name: &str) -> Option<String> {
        let sessions = self
            .agent
            .config
            .session_manager
            .list_sessions()
            .await
            .ok()?;
        sessions
            .iter()
            .rev()
            .find(|s| s.name == name)
            .map(|s| s.id.clone())
    }
}

/// Message에서 텍스트 콘텐츠만 추출.
fn extract_text_content(msg: &Message) -> String {
    use goose::conversation::message::MessageContent;
    msg.content
        .iter()
        .filter_map(|c| {
            if let MessageContent::Text(t) = c {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_content_keeps_newline_between_segments() {
        let message = Message::user().with_text("A").with_text("B").with_text("C");
        assert_eq!(extract_text_content(&message), "A\nB\nC");
    }
}
