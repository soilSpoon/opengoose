//! SandboxValidationGate — sandbox VM 안에서 cargo check/test 실행.
//! macOS HVF 전용. ValidationGate의 sandbox 대체재.

#[cfg(target_os = "macos")]
use opengoose_rig::pipeline::{Middleware, PipelineContext};
#[cfg(target_os = "macos")]
use opengoose_sandbox::{SandboxClient, SandboxPool, SandboxSession};
#[cfg(target_os = "macos")]
use tracing::{info, warn};
#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::time::Duration;

/// Validation gate that runs cargo check/test inside a sandbox microVM.
/// The host worktree is mounted read-only via virtio-fs with an overlay.
#[cfg(target_os = "macos")]
pub struct SandboxValidationGate {
    pool: Arc<SandboxPool>,
}

#[cfg(target_os = "macos")]
impl SandboxValidationGate {
    pub fn new(pool: Arc<SandboxPool>) -> Self {
        Self { pool }
    }
}

#[cfg(target_os = "macos")]
#[async_trait::async_trait]
impl Middleware for SandboxValidationGate {
    async fn validate(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<Option<String>> {
        let work_dir = ctx.work_dir.to_path_buf();
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || run_sandbox_validation(&pool, &work_dir))
            .await
            .map_err(|e| anyhow::anyhow!("sandbox task join error: {e}"))?
    }
}

/// Blocking sandbox validation: detect project type → mount worktree → run checks.
#[cfg(target_os = "macos")]
fn run_sandbox_validation(
    pool: &SandboxPool,
    work_dir: &std::path::Path,
) -> anyhow::Result<Option<String>> {
    // Detect project type — same logic as host middleware::post_execute
    let is_cargo = work_dir.join("Cargo.toml").exists();
    let is_npm = work_dir.join("package.json").exists();

    if !is_cargo && !is_npm {
        info!("no project files, skipping sandbox validation");
        return Ok(None); // No project files → pass
    }

    info!(work_dir = %work_dir.display(), "starting sandbox session");

    let client = SandboxClient::new_with_pool(pool);
    let mut session = client
        .start(work_dir)
        .map_err(|e| anyhow::anyhow!("sandbox start: {e}"))?;

    info!(project_type = if is_cargo { "cargo" } else { "npm" }, "running validation in sandbox");

    let result = if is_cargo {
        run_cargo_in_sandbox(&mut session)
    } else {
        run_npm_in_sandbox(&mut session)
    };

    match &result {
        Ok(None) => info!("sandbox validation passed"),
        Ok(Some(err)) => warn!(error = %err, "sandbox validation failed"),
        Err(_) => {} // caller handles this
    }

    result
}

#[cfg(target_os = "macos")]
fn run_cargo_in_sandbox(session: &mut SandboxSession) -> anyhow::Result<Option<String>> {
    let check = session
        .exec_with_timeout(
            "cargo",
            &["check", "--message-format=short"],
            Duration::from_secs(120),
        )
        .map_err(|e| anyhow::anyhow!("sandbox exec: {e}"))?;

    if check.status != 0 {
        let detail = if check.stdout.is_empty() {
            check.stderr
        } else {
            format!("{}\n{}", check.stdout, check.stderr)
        };
        return Ok(Some(format!("cargo check failed:\n{detail}")));
    }

    let test = session
        .exec_with_timeout("cargo", &["test"], Duration::from_secs(300))
        .map_err(|e| anyhow::anyhow!("sandbox exec: {e}"))?;

    if test.status != 0 {
        let detail = if test.stdout.is_empty() {
            test.stderr
        } else {
            format!("{}\n{}", test.stdout, test.stderr)
        };
        return Ok(Some(format!("cargo test failed:\n{detail}")));
    }

    Ok(None)
}

#[cfg(target_os = "macos")]
fn run_npm_in_sandbox(session: &mut SandboxSession) -> anyhow::Result<Option<String>> {
    let test = session
        .exec_with_timeout(
            "npm",
            &["test", "--", "--passWithNoTests"],
            Duration::from_secs(300),
        )
        .map_err(|e| anyhow::anyhow!("sandbox exec: {e}"))?;

    if test.status != 0 {
        let detail = if test.stdout.is_empty() {
            test.stderr
        } else {
            format!("{}\n{}", test.stdout, test.stderr)
        };
        return Ok(Some(format!("npm test failed:\n{detail}")));
    }

    Ok(None)
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;
    use opengoose_board::Board;
    use opengoose_board::work_item::{RigId, WorkItem, Status};
    use opengoose_board::Priority;
    use opengoose_rig::pipeline::{Middleware, PipelineContext};

    fn test_work_item() -> WorkItem {
        WorkItem {
            id: 1,
            title: "test".into(),
            description: String::new(),
            created_by: RigId::new("u"),
            created_at: chrono::Utc::now(),
            status: Status::Claimed,
            priority: Priority::P1,
            tags: vec![],
            claimed_by: Some(RigId::new("w")),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    #[ignore] // Requires macOS HVF — run with `cargo test -- --ignored`
    async fn sandbox_validation_passes_for_empty_dir() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let pool = Arc::new(SandboxPool::new());
        let gate = SandboxValidationGate::new(pool);

        let agent = goose::agents::Agent::new();
        let board = Board::in_memory().await.expect("board");
        let item = test_work_item();
        let ctx = PipelineContext {
            agent: &agent,
            work_dir: tmp.path(),
            rig_id: &RigId::new("w"),
            board: &board,
            item: &item,
        };

        let result = gate.validate(&ctx).await.expect("validate");
        assert!(result.is_none(), "empty dir should pass: no project files");
    }

    #[tokio::test]
    #[ignore] // Requires macOS HVF
    async fn sandbox_validation_fails_for_broken_cargo_project() {
        let tmp = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"broken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write");

        let pool = Arc::new(SandboxPool::new());
        let gate = SandboxValidationGate::new(pool);

        let agent = goose::agents::Agent::new();
        let board = Board::in_memory().await.expect("board");
        let item = test_work_item();
        let ctx = PipelineContext {
            agent: &agent,
            work_dir: tmp.path(),
            rig_id: &RigId::new("w"),
            board: &board,
            item: &item,
        };

        let result = gate.validate(&ctx).await.expect("validate");
        assert!(result.is_some(), "broken cargo project should fail");
        assert!(result.unwrap().contains("cargo check failed"));
    }

    #[tokio::test]
    #[ignore] // Requires macOS HVF
    async fn sandbox_validation_passes_for_valid_cargo_project() {
        let tmp = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"valid\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write");
        std::fs::create_dir_all(tmp.path().join("src")).expect("mkdir");
        std::fs::write(tmp.path().join("src/lib.rs"), "").expect("write");

        let pool = Arc::new(SandboxPool::new());
        let gate = SandboxValidationGate::new(pool);

        let agent = goose::agents::Agent::new();
        let board = Board::in_memory().await.expect("board");
        let item = test_work_item();
        let ctx = PipelineContext {
            agent: &agent,
            work_dir: tmp.path(),
            rig_id: &RigId::new("w"),
            board: &board,
            item: &item,
        };

        let result = gate.validate(&ctx).await.expect("validate");
        assert!(result.is_none(), "valid cargo project should pass");
    }
}
