//! SandboxValidationGate — sandbox VM 안에서 cargo check/test 실행.
//! macOS HVF 전용. ValidationGate의 sandbox 대체재.

#[cfg(target_os = "macos")]
use opengoose_rig::pipeline::{Middleware, PipelineContext};
#[cfg(target_os = "macos")]
use opengoose_sandbox::{SandboxClient, SandboxPool, SandboxSession};
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
        return Ok(None); // No project files → pass
    }

    let client = SandboxClient::new_with_pool(pool);
    let mut session = client
        .start(work_dir)
        .map_err(|e| anyhow::anyhow!("sandbox start: {e}"))?;

    if is_cargo {
        return run_cargo_in_sandbox(&mut session);
    }

    run_npm_in_sandbox(&mut session)
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
