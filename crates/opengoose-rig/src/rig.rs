// Rig<M: WorkMode> — Strategy 패턴으로 Operator/Worker 공유 구조.
//
// Operator = Rig<ChatMode>: 영속 세션, 직접 대화. Board를 거치지 않음.
// Worker   = Rig<TaskMode>: 작업당 세션, pull loop. Board에서 claim → execute → submit.
//
// 공유: process() — Agent.reply() 호출 + 스트림 소비.
// 차이: WorkMode가 세션 관리를 결정.

use crate::conversation_log;
use crate::pipeline::{Middleware, PipelineContext};
use crate::work_mode::{ChatMode, EvolveMode, TaskMode, WorkInput, WorkMode, task_session_id};
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent};
use goose::conversation::message::Message;
use opengoose_board::Board;
use opengoose_board::work_item::{RigId, WorkItem};
use std::path::Path;
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
    middleware: Vec<Arc<dyn Middleware>>,
}

/// Operator: 사용자 대면. 영속 세션. Board를 거치지 않고 직접 대화.
pub type Operator = Rig<ChatMode>;

/// Worker: Board 워커. Pull loop. 작업당 세션.
pub type Worker = Rig<TaskMode>;

/// Evolver: stamp 감지 → 스킬 생성. 분석당 세션.
pub type Evolver = Rig<EvolveMode>;

// ── 공유 (모든 WorkMode) ─────────────────────────────────────

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
            middleware: vec![],
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

        // Phase 0: Sweep — 크래시로 남은 고아 worktree 정리
        let repo_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
        crate::worktree::sweep_orphaned_worktrees(&repo_dir, &self.id, board, None).await;

        // Phase 1: Resume — 이전에 claim한 아이템 처리
        let stale = board.claimed_by(&self.id).await.unwrap_or_default();
        if !stale.is_empty() {
            info!(rig = %self.id, count = stale.len(), "resuming previously claimed items");
        }
        for item in &stale {
            if self.cancel.is_cancelled() {
                break;
            }
            self.process_claimed_item(item, board, &repo_dir).await;
        }

        loop {
            // 1. 관심 먼저 등록 — 이 시점 이후의 모든 알림을 캡처
            let notify = board.notify_handle();
            let notified = notify.notified();

            // 2. 준비된 항목 확인 + 실행
            match self.try_claim_and_execute(&repo_dir).await {
                Ok(true) => continue, // 작업 발견, 즉시 추가 확인
                Ok(false) => {}       // 작업 없음, 대기로 이동
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
    /// SQLite 트랜잭션으로 claim하여 원자성과 AlreadyClaimed 검증 보장.
    async fn try_claim_and_execute(&self, repo_dir: &Path) -> anyhow::Result<bool> {
        let board = self.board.as_ref().expect("Worker must have a board");

        let ready = board.ready().await?;
        if ready.is_empty() {
            return Ok(false);
        }

        // 첫 번째 claim 가능한 candidate를 찾아 실행
        let claimed = self.try_claim_first(board, &ready).await?;
        let Some(item) = claimed else {
            return Ok(true); // 모두 선점됨 — 즉시 재시도
        };

        info!(rig = %self.id, item_id = item.id, title = %item.title, "claimed work item");
        self.process_claimed_item(&item, board, repo_dir).await;
        Ok(true)
    }

    /// ready 목록에서 첫 번째 claim 가능한 아이템을 가져간다.
    /// AlreadyClaimed는 skip, 다른 에러는 전파.
    async fn try_claim_first(
        &self,
        board: &Arc<Board>,
        candidates: &[WorkItem],
    ) -> anyhow::Result<Option<WorkItem>> {
        for candidate in candidates {
            match board.claim(candidate.id, &self.id).await {
                Ok(claimed) => return Ok(Some(claimed)),
                Err(opengoose_board::BoardError::AlreadyClaimed { .. }) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(None)
    }

    /// claim된 아이템을 처리. 세션 조회/생성 → process → submit or abandon.
    /// 에러는 내부에서 처리하고 호출자에게 전파하지 않음.
    async fn process_claimed_item(&self, item: &WorkItem, board: &Arc<Board>, repo_dir: &Path) {
        // Phase 1: Worktree 확보
        let guard = match self.acquire_worktree(repo_dir, item.id) {
            Ok(guard) => guard,
            Err(e) => {
                warn!(rig = %self.id, item_id = item.id, error = %e, "failed to acquire worktree, abandoning");
                board.abandon(item.id).await.ok();
                return;
            }
        };

        // Phase 2: Middleware hydration
        let pipeline_ctx = PipelineContext {
            agent: &self.agent,
            work_dir: &guard.path,
            rig_id: &self.id,
            board: board.as_ref(),
            item,
        };
        if let Err(e) = pipeline_ctx.run_on_start(&self.middleware).await {
            warn!(rig = %self.id, item_id = item.id, error = %e, "middleware on_start failed, abandoning");
            board.abandon(item.id).await.ok();
            guard.remove().await;
            return;
        }

        // Phase 3: Session 확보
        let session_name = task_session_id(item.id);
        let (session_id, resuming) = match self.resolve_session(&session_name, &guard.path).await {
            Ok(result) => result,
            Err(e) => {
                warn!(rig = %self.id, item_id = item.id, error = %e, "failed to resolve session, abandoning");
                board.abandon(item.id).await.ok();
                guard.remove().await;
                return;
            }
        };

        // Phase 4: Execute with bounded retry
        let prompt = if resuming {
            format!("Continue working on item #{}: {}", item.id, item.title)
        } else {
            format!(
                "Work item #{}: {}\n\n{}",
                item.id, item.title, item.description
            )
        };

        let keep = self
            .execute_with_retry(item, board, &pipeline_ctx, &session_id, &prompt)
            .await;

        // Phase 5: Cleanup — stuck 상태면 worktree 보존
        if !keep {
            guard.remove().await;
        }
    }

    /// Worktree 확보: attach(기존) → create(신규).
    fn acquire_worktree(
        &self,
        repo_dir: &Path,
        item_id: i64,
    ) -> anyhow::Result<crate::worktree::WorktreeGuard> {
        if let Some(guard) =
            crate::worktree::WorktreeGuard::attach(repo_dir, &self.id, item_id, None)
        {
            info!(rig = %self.id, item_id, "attached to existing worktree");
            return Ok(guard);
        }
        let guard = crate::worktree::WorktreeGuard::create(repo_dir, &self.id, item_id, None)?;
        info!(rig = %self.id, item_id, path = %guard.path.display(), "created worktree");
        Ok(guard)
    }

    /// 세션 조회(기존) 또는 생성(신규). (session_id, resuming) 반환.
    async fn resolve_session(
        &self,
        session_name: &str,
        work_dir: &Path,
    ) -> anyhow::Result<(String, bool)> {
        if let Some(id) = self.find_session_by_name(session_name).await {
            info!(rig = %self.id, "resuming existing session");
            return Ok((id, true));
        }
        let session = self
            .agent
            .config
            .session_manager
            .create_session(
                work_dir.to_path_buf(),
                session_name.to_string(),
                goose::session::session_manager::SessionType::User,
                goose::config::goose_mode::GooseMode::Auto,
            )
            .await?;
        Ok((session.id, false))
    }

    /// Bounded retry loop: process → validate → (submit | retry | stuck).
    /// Returns `true` if the worktree should be kept (e.g. stuck items for debugging).
    async fn execute_with_retry(
        &self,
        item: &WorkItem,
        board: &Arc<Board>,
        pipeline_ctx: &PipelineContext<'_>,
        session_id: &str,
        initial_prompt: &str,
    ) -> bool {
        const MAX_RETRIES: u32 = 2;

        let input =
            WorkInput::task(initial_prompt, item.id).with_session_id(session_id.to_string());
        let mut last_result = self.process(input).await;

        for attempt in 0..=MAX_RETRIES {
            // LLM 실패 → 즉시 중단 (retry 대상 아님)
            if let Err(ref e) = last_result {
                warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
                board.abandon(item.id).await.ok();
                return false;
            }

            // LLM 성공 → 검증
            let validation = match pipeline_ctx.run_validate(&self.middleware).await {
                Ok(v) => v,
                Err(e) => {
                    warn!(rig = %self.id, item_id = item.id, error = %e, "validation infra failed, abandoning");
                    board.abandon(item.id).await.ok();
                    return false;
                }
            };

            match validation {
                None => {
                    // 검증 통과 → submit
                    if let Err(e) = board.submit(item.id, &self.id).await {
                        warn!(rig = %self.id, item_id = item.id, error = %e, "submit failed");
                    } else {
                        info!(rig = %self.id, item_id = item.id, "submitted work item");
                    }
                    return false;
                }
                Some(ref validation_error) if attempt < MAX_RETRIES => {
                    warn!(
                        rig = %self.id, item_id = item.id,
                        attempt = attempt + 1, max = MAX_RETRIES,
                        "validation failed, retrying"
                    );
                    let fix_prompt = format!(
                        "The previous implementation failed validation. Please fix the errors:\n\n{}",
                        validation_error
                    );
                    let retry_input = WorkInput::task(fix_prompt, item.id)
                        .with_session_id(session_id.to_string());
                    last_result = self.process(retry_input).await;
                }
                Some(validation_error) => {
                    warn!(
                        rig = %self.id, item_id = item.id,
                        error = %validation_error,
                        "validation failed after {MAX_RETRIES} retries, marking stuck"
                    );
                    board.mark_stuck(item.id, &self.id).await.ok();
                    return true;
                }
            }
        }

        false
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
