// Pipeline — Middleware trait for composable Blueprint execution.
//
// on_start: LLM 호출 전 1회 실행 (컨텍스트 주입)
// post_process: LLM 호출 후 매번 실행 (검증)

use goose::agents::Agent;
use opengoose_board::work_item::{RigId, WorkItem};
use opengoose_board::Board;
use std::path::Path;

/// 미들웨어가 참조하는 파이프라인 컨텍스트. 소유권 없음.
pub struct PipelineContext<'a> {
    pub agent: &'a Agent,
    pub work_dir: &'a Path,
    pub rig_id: &'a RigId,
    pub board: &'a Board,
    pub item: &'a WorkItem,
}

/// AGENTS.md + 스킬 카탈로그 + Board prime을 시스템 프롬프트에 주입.
pub struct ContextHydrator {
    pub skill_catalog: String,
}

#[async_trait::async_trait]
impl Middleware for ContextHydrator {
    async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> {
        let all_items = ctx.board.list().await.unwrap_or_default();
        let board_prime = opengoose_board::beads::prime_summary(&all_items, ctx.rig_id);
        crate::middleware::pre_hydrate(ctx.agent, ctx.work_dir, &self.skill_catalog, &board_prime)
            .await;
        Ok(())
    }
}

/// 조합 가능한 미들웨어 trait.
///
/// on_start: LLM 호출 전 1회. 시스템 프롬프트 확장 등.
/// post_process: LLM 호출 후 매번. None = 통과, Some(err) = 실패.
#[async_trait::async_trait]
pub trait Middleware: Send + Sync {
    async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> {
        let _ = ctx;
        Ok(())
    }

    async fn post_process(&self, ctx: &PipelineContext<'_>) -> Option<String> {
        let _ = ctx;
        None
    }
}
