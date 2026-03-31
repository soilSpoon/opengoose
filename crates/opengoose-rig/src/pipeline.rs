// Pipeline — Middleware trait for composable Blueprint execution.
//
// on_start: LLM 호출 전 1회 실행 (컨텍스트 주입)
// validate: LLM 호출 후 매번 실행 (검증)

use goose::agents::Agent;
use opengoose_board::Board;
use opengoose_board::work_item::{RigId, WorkItem};
use std::path::Path;
use std::sync::Arc;

/// Borrowed context passed to each [`Middleware`] hook.
///
/// Contains references to the agent, work directory, rig identity,
/// board, and current work item. Middleware never takes ownership.
///
/// ```no_run
/// use opengoose_rig::pipeline::PipelineContext;
/// use opengoose_board::work_item::RigId;
/// use opengoose_board::Board;
/// use goose::agents::Agent;
/// use std::path::Path;
///
/// // PipelineContext is constructed by the Rig during execution:
/// // let ctx = PipelineContext {
/// //     agent: &agent,
/// //     work_dir: Path::new("/repo"),
/// //     rig_id: &RigId::new("worker-1"),
/// //     board: &board,
/// //     item: &work_item,
/// // };
/// ```
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
    pub async fn run_validate(
        &self,
        middleware: &[Arc<dyn Middleware>],
    ) -> anyhow::Result<Option<String>> {
        for mw in middleware {
            if let Some(err) = mw.validate(self).await? {
                return Ok(Some(err));
            }
        }
        Ok(None)
    }
}

/// Composable middleware trait for the pipeline.
///
/// - `on_start`: called once before the LLM call (e.g. system prompt extension).
/// - `validate`: called after each LLM call. `Ok(None)` = pass, `Ok(Some(msg))` = validation failure.
///
/// Both methods have default no-op implementations, so you only need to override what you use.
///
/// # Example
///
/// ```no_run
/// use opengoose_rig::pipeline::{Middleware, PipelineContext};
///
/// struct LoggingMiddleware;
///
/// #[async_trait::async_trait]
/// impl Middleware for LoggingMiddleware {
///     async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> {
///         println!("Starting work in {:?}", ctx.work_dir);
///         Ok(())
///     }
///
///     async fn validate(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<Option<String>> {
///         // Return Some("error message") to signal validation failure
///         Ok(None)
///     }
/// }
/// ```
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

/// Hydrates the system prompt with AGENTS.md, skill catalog, and Board summary.
///
/// Runs during `on_start` to inject context before the LLM processes work.
///
/// ```
/// use opengoose_rig::pipeline::ContextHydrator;
///
/// let hydrator = ContextHydrator {
///     skill_catalog: "## Skills\n- code-review\n- test-gen".to_string(),
/// };
/// assert!(!hydrator.skill_catalog.is_empty());
/// ```
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

/// Runs `cargo check` + `cargo test` (or `npm test`) after LLM execution.
///
/// Returns `Ok(Some(error))` when validation fails, triggering a retry.
///
/// ```
/// use opengoose_rig::pipeline::ValidationGate;
///
/// let gate = ValidationGate;
/// // ValidationGate is a unit struct -- it reads the work_dir
/// // from PipelineContext at runtime to decide which checks to run.
/// ```
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
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let agent = goose::agents::Agent::new();
        let board = Board::in_memory()
            .await
            .expect("in-memory board should initialize");
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
            parent_id: None,
        };
        let ctx = PipelineContext {
            agent: &agent,
            work_dir: tmp.path(),
            rig_id: &RigId::new("w"),
            board: &board,
            item: &item,
        };
        let gate = ValidationGate;
        let result = gate
            .validate(&ctx)
            .await
            .expect("async operation should succeed");
        assert!(result.is_none(), "empty dir should pass validation");
    }
}
