// Rig — 영속 에이전트 정체성 + Pull 루프
//
// loop { board.wait_for_claimable() → claim → agent.reply() → submit }
//
// Phase 2: Rig struct + Board 연결.
// Agent 생성과 reply() 호출은 Goose Provider 설정이 필요하므로
// 실제 LLM 연결은 Phase 3 (CLI)에서 완성.

use crate::mcp_tools::BoardClient;
use goose::agents::extension::ExtensionConfig;
use goose::agents::{Agent, AgentEvent};
use goose::conversation::message::Message;
use opengoose_board::work_item::RigId;
use opengoose_board::Board;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Rig = 영속 에이전트 정체성 + Pull 루프.
///
/// Board에서 작업을 자발적으로 가져가서 Goose Agent로 처리한다.
pub struct Rig {
    pub id: RigId,
    board: Arc<Mutex<Board>>,
    agent: Agent,
    cancel: CancellationToken,
}

impl Rig {
    /// Rig 생성. Agent에 Board 도구를 자동 주입.
    pub async fn new(id: RigId, board: Arc<Mutex<Board>>, agent: Agent) -> Self {
        // Board 도구를 Agent에 주입
        let board_client = Arc::new(BoardClient::new(Arc::clone(&board), id.clone()));
        let config = ExtensionConfig::Platform {
            name: "board".into(),
            description: "Wanted Board — work items, claim, submit".into(),
            display_name: Some("Board".into()),
            bundled: None,
            available_tools: vec![],
        };
        agent
            .extension_manager
            .add_client("board".into(), config, board_client, None, None)
            .await;

        Self {
            id,
            board,
            agent,
            cancel: CancellationToken::new(),
        }
    }

    /// Pull loop 시작. cancel_token으로 중단.
    pub async fn run(&self) {
        info!(rig = %self.id, "rig started, waiting for work");

        loop {
            // notify handle을 미리 얻어서 lock 수명 문제 회피
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
                    info!(rig = %self.id, "rig cancelled");
                    break;
                }
            }
        }
    }

    /// 보드에서 작업을 가져가서 실행.
    async fn try_claim_and_execute(&self) -> anyhow::Result<()> {
        let mut board = self.board.lock().await;
        let ready = board.ready();

        let Some(item) = ready.first() else {
            return Ok(()); // 가져갈 게 없음
        };

        let item = board.claim(item.id, &self.id)?;
        info!(rig = %self.id, item_id = item.id, title = %item.title, "claimed work item");
        drop(board); // lock 해제 후 agent 실행

        // 작업 내용을 메시지로 변환
        let message = Message::user().with_text(format!(
            "Work item #{}: {}\n\n{}",
            item.id, item.title, item.description
        ));

        let session_config = goose::agents::SessionConfig {
            id: format!("rig-{}-{}", self.id, item.id),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        // Goose Agent 실행
        let stream = self
            .agent
            .reply(message, session_config, Some(self.cancel.clone()))
            .await?;

        // 스트림 소비
        use futures::StreamExt;
        tokio::pin!(stream);
        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Message(msg)) => {
                    // Phase 3에서 CLI로 스트리밍
                    tracing::debug!(rig = %self.id, "agent message: {:?}", msg.role);
                }
                Err(e) => {
                    warn!(rig = %self.id, error = %e, "agent error");
                    break;
                }
                _ => {}
            }
        }

        // 완료 제출
        let mut board = self.board.lock().await;
        board.submit(item.id, &self.id)?;
        info!(rig = %self.id, item_id = item.id, "submitted work item");

        Ok(())
    }

    /// Rig 중단.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }
}
