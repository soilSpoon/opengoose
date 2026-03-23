// Pipeline — Middleware trait for composable Blueprint execution.
//
// on_start: LLM 호출 전 1회 실행 (컨텍스트 주입)
// validate: LLM 호출 후 매번 실행 (검증)

use goose::agents::Agent;
use opengoose_board::work_item::{RigId, WorkItem};
use opengoose_board::Board;
use std::path::Path;
use std::sync::Arc;

/// 미들웨어가 참조하는 파이프라인 컨텍스트. 소유권 없음.
pub struct PipelineContext<'a> {
    pub agent: &'a Agent,
    pub work_dir: &'a Path,
    pub rig_id: &'a RigId,
    pub board: &'a Board,
    pub item: &'a WorkItem,
}

impl<'a> PipelineContext<'a> {
    /// 모든 미들웨어의 on_start 실행. 하나라도 실패하면 즉시 Err 반환.
    pub async fn run_on_start(&self, middleware: &[Arc<dyn Middleware>]) -> anyhow::Result<()> {
        for mw in middleware {
            mw.on_start(self).await?;
        }
        Ok(())
    }

    /// 모든 미들웨어의 validate 실행. 첫 번째 검증 실패 시 Ok(Some) 반환.
    /// 인프라 실패 시 Err 반환.
    pub async fn run_validate(&self, middleware: &[Arc<dyn Middleware>]) -> anyhow::Result<Option<String>> {
        for mw in middleware {
            if let Some(err) = mw.validate(self).await? {
                return Ok(Some(err));
            }
        }
        Ok(None)
    }
}

/// 조합 가능한 미들웨어 trait.
///
/// on_start: LLM 호출 전 1회. 시스템 프롬프트 확장 등.
/// validate: LLM 호출 후 매번. Ok(None) = 통과, Ok(Some) = 검증 실패, Err = 인프라 실패.
#[async_trait::async_trait]
pub trait Middleware: Send + Sync {
    async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> {
        let _ = ctx;
        Ok(())
    }

    /// 검증 실행. Ok(None) = 통과, Ok(Some(msg)) = 검증 실패, Err = 인프라 실패.
    async fn validate(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<Option<String>> {
        let _ = ctx;
        Ok(None)
    }
}

/// AGENTS.md + 스킬 카탈로그 + Board prime을 시스템 프롬프트에 주입.
pub struct ContextHydrator {
    pub skill_catalog: String,
}

#[async_trait::async_trait]
impl Middleware for ContextHydrator {
    async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> {
        let all_items = ctx.board.list().await?;
        let board_prime = opengoose_board::beads::prime_summary(&all_items, ctx.rig_id);
        crate::middleware::pre_hydrate(ctx.agent, ctx.work_dir, &self.skill_catalog, &board_prime)
            .await;
        Ok(())
    }
}

/// cargo check + cargo test 자동 실행. 실패 시 에러 메시지 반환.
pub struct ValidationGate;

#[async_trait::async_trait]
impl Middleware for ValidationGate {
    async fn validate(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<Option<String>> {
        crate::middleware::post_execute(ctx.work_dir).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_board::Priority;

    #[tokio::test]
    async fn validation_gate_returns_ok_none_for_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let agent = goose::agents::Agent::new();
        let board = Board::in_memory().await.unwrap();
        let item = WorkItem {
            id: 1,
            title: "test".into(),
            description: String::new(),
            created_by: RigId::new("u"),
            created_at: chrono::Utc::now(),
            status: opengoose_board::work_item::Status::Claimed,
            priority: Priority::P1,
            tags: vec![],
            claimed_by: Some(RigId::new("w")),
            updated_at: chrono::Utc::now(),
        };
        let ctx = PipelineContext {
            agent: &agent,
            work_dir: tmp.path(),
            rig_id: &RigId::new("w"),
            board: &board,
            item: &item,
        };
        let gate = ValidationGate;
        let result = gate.validate(&ctx).await.unwrap();
        assert!(result.is_none(), "empty dir should pass validation");
    }
}
