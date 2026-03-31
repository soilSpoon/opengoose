# Sandbox ↔ Worker Integration Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Worker의 ValidationGate가 sandbox microVM 안에서 `cargo check`/`cargo test`를 실행하도록 통합하여, 에이전트가 작성한 코드의 검증을 격리된 환경에서 수행한다.

**Architecture:** `SandboxValidationGate`는 기존 `ValidationGate`과 동일한 `Middleware` trait을 구현하되, `post_execute()`가 호스트 프로세스를 직접 실행하는 대신 `SandboxClient`를 통해 VM 안에서 명령을 실행한다. `SandboxPool`은 `Arc`로 공유되어 VM 재사용을 보장한다. Runtime에서 `--sandbox` CLI 플래그로 두 gate 중 하나를 선택한다.

**Tech Stack:** Rust, opengoose-sandbox (HVF microVM), opengoose-rig (Middleware trait), clap (CLI)

---

### Task 1: opengoose-rig에 opengoose-sandbox 의존성 추가 (optional)

**Files:**
- Modify: `crates/opengoose-rig/Cargo.toml`
- Modify: `Cargo.toml` (workspace)

opengoose-sandbox는 macOS 전용(`#[cfg(target_os = "macos")]`)이므로, opengoose-rig가 직접 의존하면 크로스 플랫폼 빌드가 깨진다. 대신 opengoose-sandbox의 `SandboxClient`/`SandboxPool`을 opengoose (바이너리 크레이트)에서 조립하고, Middleware trait의 동적 디스패치로 주입한다. **따라서 opengoose-rig에는 sandbox 의존성을 추가하지 않는다.**

- [ ] **Step 1: 확인 — opengoose-rig/Cargo.toml에 sandbox 의존성이 없음을 확인**

아무 변경 없음. 이 태스크는 설계 결정 기록용.

- [ ] **Step 2: opengoose (바이너리) Cargo.toml에 sandbox 의존성 확인**

`crates/opengoose/Cargo.toml`에 이미 `opengoose-sandbox` 의존성이 있는지 확인.

Run: `grep sandbox crates/opengoose/Cargo.toml`

있으면 다음 태스크로 진행. 없으면 추가:

```toml
[dependencies]
opengoose-sandbox = { workspace = true }
```

그리고 workspace `Cargo.toml`에도:

```toml
[workspace.dependencies]
opengoose-sandbox = { path = "crates/opengoose-sandbox" }
```

- [ ] **Step 3: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/opengoose/Cargo.toml
git commit -m "chore: ensure opengoose binary depends on opengoose-sandbox"
```

---

### Task 2: SandboxValidationGate 구조체 + Middleware impl

**Files:**
- Create: `crates/opengoose/src/sandbox_gate.rs`
- Modify: `crates/opengoose/src/main.rs` (mod 선언)

SandboxValidationGate는 `opengoose-rig::pipeline::Middleware`를 구현한다. `SandboxPool`을 `Arc`로 소유하고, `validate()` 에서 sandbox session을 열어 검증 커맨드를 실행한다.

- [ ] **Step 1: sandbox_gate.rs 파일 생성 — 구조체 + 빈 Middleware impl**

Create `crates/opengoose/src/sandbox_gate.rs`:

```rust
//! SandboxValidationGate — sandbox VM 안에서 cargo check/test 실행.
//! macOS HVF 전용. ValidationGate의 sandbox 대체재.

#[cfg(target_os = "macos")]
use opengoose_rig::pipeline::{Middleware, PipelineContext};
#[cfg(target_os = "macos")]
use opengoose_sandbox::{SandboxClient, SandboxPool};
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

/// Blocking sandbox validation: mount worktree → cargo check → cargo test.
#[cfg(target_os = "macos")]
fn run_sandbox_validation(
    pool: &SandboxPool,
    work_dir: &std::path::Path,
) -> anyhow::Result<Option<String>> {
    let client = SandboxClient::new_with_pool(pool);
    let mut session = client
        .start(work_dir)
        .map_err(|e| anyhow::anyhow!("sandbox start: {e}"))?;

    // cargo check
    let check_timeout = Duration::from_secs(120);
    let check = session
        .exec_with_timeout("cargo", &["check", "--message-format=short"], check_timeout)
        .map_err(|e| anyhow::anyhow!("sandbox exec: {e}"))?;

    if check.status != 0 {
        let detail = if check.stdout.is_empty() {
            check.stderr
        } else {
            format!("{}\n{}", check.stdout, check.stderr)
        };
        return Ok(Some(format!("cargo check failed:\n{detail}")));
    }

    // cargo test
    let test_timeout = Duration::from_secs(300);
    let test = session
        .exec_with_timeout("cargo", &["test"], test_timeout)
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
```

- [ ] **Step 2: main.rs에 mod 선언 추가**

`crates/opengoose/src/main.rs`에 추가:

```rust
#[cfg(target_os = "macos")]
mod sandbox_gate;
```

- [ ] **Step 3: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose/src/sandbox_gate.rs crates/opengoose/src/main.rs
git commit -m "feat(sandbox): add SandboxValidationGate middleware"
```

---

### Task 3: SandboxClient에 pool 주입 생성자 추가

**Files:**
- Modify: `crates/opengoose-sandbox/src/client.rs`

현재 `SandboxClient::new()`는 내부에 `SandboxPool`을 생성한다. 외부에서 공유 pool을 주입받는 생성자가 필요하다 (Worker 간 VM 재사용).

- [ ] **Step 1: 테스트 작성 (컴파일 확인)**

`crates/opengoose-sandbox/src/client.rs` 하단에 테스트 모듈 추가:

```rust
#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn new_with_pool_shares_pool_reference() {
        let pool = SandboxPool::new();
        let client = SandboxClient::new_with_pool(&pool);
        // client was created without error — pool reference is valid
        let _ = client;
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p opengoose-sandbox new_with_pool`
Expected: FAIL — `new_with_pool` 메서드 없음

- [ ] **Step 3: new_with_pool 구현**

`SandboxClient`의 `pool` 필드를 `SandboxPool`에서 lifetime 참조로 바꾸면 `SandboxSession`까지 연쇄적으로 변경이 필요해진다. 대신 `pool` 필드 타입은 유지하되, 외부 pool에서 VM을 acquire하는 별도 생성자를 추가한다.

실제로 `SandboxClient`가 pool을 소유하는 것이 아니라 외부 `&SandboxPool`을 사용하는 free function 접근이 더 깔끔하다. 하지만 기존 API를 깨지 않기 위해, `SandboxClient`에 `from_pool()` 패턴을 사용한다:

`crates/opengoose-sandbox/src/client.rs`에서 `SandboxClient` impl 블록에 추가:

```rust
#[cfg(target_os = "macos")]
impl SandboxClient {
    // ... 기존 new() ...

    /// Create a client that borrows an external pool.
    /// The pool's VM is reused across calls (sub-ms fork).
    pub fn new_with_pool(pool: &SandboxPool) -> SandboxClientRef<'_> {
        SandboxClientRef { pool }
    }
}

/// A sandbox client borrowing an external pool (no ownership).
#[cfg(target_os = "macos")]
pub struct SandboxClientRef<'a> {
    pool: &'a SandboxPool,
}

#[cfg(target_os = "macos")]
impl SandboxClientRef<'_> {
    /// Start a sandbox session for the given host worktree directory.
    pub fn start(&self, worktree: &Path) -> Result<SandboxSession> {
        let mut vm = self.pool.acquire()?;
        vm.mount_virtio_fs(worktree);

        let mount_result = vm.exec_raw("mount_workspace", &[], DEFAULT_TIMEOUT)?;
        if mount_result.status != 0 {
            return Err(SandboxError::Exec(format!(
                "workspace mount failed: {}",
                mount_result.stderr
            )));
        }

        Ok(SandboxSession {
            vm,
            worktree: worktree.to_path_buf(),
        })
    }
}
```

- [ ] **Step 4: lib.rs에서 SandboxClientRef export**

`crates/opengoose-sandbox/src/lib.rs`에 추가:

```rust
#[cfg(target_os = "macos")]
pub use client::{ApplyResult, SandboxClient, SandboxClientRef, SandboxSession};
```

(기존 `pub use client::{ApplyResult, SandboxClient, SandboxSession};` 줄을 교체)

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test -p opengoose-sandbox new_with_pool`
Expected: PASS

- [ ] **Step 6: start() 로직 중복 제거 — 헬퍼 함수 추출**

`SandboxClient::start()`와 `SandboxClientRef::start()`가 동일한 mount 로직을 공유한다. free function으로 추출:

```rust
#[cfg(target_os = "macos")]
fn start_session(pool: &SandboxPool, worktree: &Path) -> Result<SandboxSession> {
    let mut vm = pool.acquire()?;
    vm.mount_virtio_fs(worktree);

    let mount_result = vm.exec_raw("mount_workspace", &[], DEFAULT_TIMEOUT)?;
    if mount_result.status != 0 {
        return Err(SandboxError::Exec(format!(
            "workspace mount failed: {}",
            mount_result.stderr
        )));
    }

    Ok(SandboxSession {
        vm,
        worktree: worktree.to_path_buf(),
    })
}
```

그리고 양쪽 `start()`를 `start_session()`으로 위임:

```rust
// SandboxClient::start
pub fn start(&self, worktree: &Path) -> Result<SandboxSession> {
    start_session(&self.pool, worktree)
}

// SandboxClientRef::start
pub fn start(&self, worktree: &Path) -> Result<SandboxSession> {
    start_session(self.pool, worktree)
}
```

- [ ] **Step 7: cargo check + 기존 테스트 통과 확인**

Run: `cargo check && cargo test -p opengoose-sandbox`
Expected: 전부 PASS

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose-sandbox/src/client.rs crates/opengoose-sandbox/src/lib.rs
git commit -m "feat(sandbox): add SandboxClientRef for external pool sharing"
```

---

### Task 4: SandboxValidationGate에서 SandboxClientRef 사용하도록 수정

**Files:**
- Modify: `crates/opengoose/src/sandbox_gate.rs`

Task 3에서 만든 `SandboxClientRef`를 사용하도록 `run_sandbox_validation`을 수정한다.

- [ ] **Step 1: sandbox_gate.rs 수정**

`run_sandbox_validation` 함수에서 `SandboxClient::new_with_pool` → `SandboxClientRef` 사용:

```rust
#[cfg(target_os = "macos")]
fn run_sandbox_validation(
    pool: &SandboxPool,
    work_dir: &std::path::Path,
) -> anyhow::Result<Option<String>> {
    let client = SandboxClient::new_with_pool(pool);
    let mut session = client
        .start(work_dir)
        .map_err(|e| anyhow::anyhow!("sandbox start: {e}"))?;

    // cargo check
    let check_timeout = Duration::from_secs(120);
    let check = session
        .exec_with_timeout("cargo", &["check", "--message-format=short"], check_timeout)
        .map_err(|e| anyhow::anyhow!("sandbox exec: {e}"))?;

    if check.status != 0 {
        let detail = if check.stdout.is_empty() {
            check.stderr
        } else {
            format!("{}\n{}", check.stdout, check.stderr)
        };
        return Ok(Some(format!("cargo check failed:\n{detail}")));
    }

    // cargo test
    let test_timeout = Duration::from_secs(300);
    let test = session
        .exec_with_timeout("cargo", &["test"], test_timeout)
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
```

imports 정리 — `SandboxClient` (owned) import를 제거하고 `SandboxClient` (for `new_with_pool`)만 유지:

```rust
#[cfg(target_os = "macos")]
use opengoose_sandbox::SandboxClient;
#[cfg(target_os = "macos")]
use opengoose_sandbox::SandboxPool;
```

- [ ] **Step 2: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose/src/sandbox_gate.rs
git commit -m "feat(sandbox): wire SandboxClientRef into SandboxValidationGate"
```

---

### Task 5: --sandbox CLI 플래그 추가

**Files:**
- Modify: `crates/opengoose/src/cli/mod.rs`

- [ ] **Step 1: Cli struct에 sandbox 플래그 추가**

`crates/opengoose/src/cli/mod.rs`에서 `Cli` struct에 추가:

```rust
#[derive(Parser)]
#[command(name = "opengoose", version = "0.2.0")]
#[command(about = "Goose-native pull architecture with Wasteland-level agent autonomy")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// 웹 대시보드 포트
    #[arg(long, default_value = "1355", global = true)]
    pub port: u16,

    /// Worker 검증을 sandbox VM 안에서 실행 (macOS only)
    #[arg(long, default_value_t = false, global = true)]
    pub sandbox: bool,
}
```

- [ ] **Step 2: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 3: 기존 CLI 파싱 테스트 통과 확인**

Run: `cargo test -p opengoose parse_board`
Expected: PASS (기존 테스트에 영향 없음, default_value_t = false)

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose/src/cli/mod.rs
git commit -m "feat(cli): add --sandbox flag for sandboxed validation"
```

---

### Task 6: Runtime에서 --sandbox에 따라 middleware 선택

**Files:**
- Modify: `crates/opengoose/src/runtime.rs`
- Modify: `crates/opengoose/src/cli/commands.rs`

init_runtime에 sandbox 여부를 전달하여, SandboxValidationGate 또는 ValidationGate를 선택한다.

- [ ] **Step 1: init_runtime 시그니처에 sandbox 파라미터 추가**

`crates/opengoose/src/runtime.rs` 수정:

```rust
use opengoose_rig::pipeline::{ContextHydrator, Middleware, ValidationGate};

/// Stand up the full runtime: Board, web dashboard, Evolver, and Worker.
pub async fn init_runtime(port: u16, sandbox: bool) -> Result<Runtime> {
    let board = Arc::new(Board::connect(&crate::db_url()).await?);
    web::spawn_server(Arc::clone(&board), port).await?;

    // Evolver
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(opengoose_evolver::run(Arc::clone(&board), stamp_notify));

    // Build validation middleware
    let validation: Arc<dyn Middleware> = if sandbox {
        #[cfg(target_os = "macos")]
        {
            let pool = Arc::new(opengoose_sandbox::SandboxPool::new());
            Arc::new(crate::sandbox_gate::SandboxValidationGate::new(pool))
        }
        #[cfg(not(target_os = "macos"))]
        {
            tracing::warn!("--sandbox is only supported on macOS, falling back to host validation");
            Arc::new(ValidationGate)
        }
    } else {
        Arc::new(ValidationGate)
    };

    // Worker
    let worker = match create_worker_agent().await {
        Ok((worker_agent, _)) => {
            let worker = Arc::new(opengoose_rig::rig::Worker::new(
                RigId::new("worker"),
                Arc::clone(&board),
                worker_agent,
                opengoose_rig::work_mode::TaskMode,
                vec![
                    Arc::new(ContextHydrator {
                        skill_catalog: String::new(),
                    }),
                    validation,
                ],
            ));
            let worker_handle = Arc::clone(&worker);
            tokio::spawn(async move { worker_handle.run().await });
            Some(worker)
        }
        Err(e) => {
            tracing::warn!(error = %e, "worker agent creation failed, running without worker");
            None
        }
    };

    Ok(Runtime { board, worker })
}
```

- [ ] **Step 2: cli/commands.rs에서 init_runtime 호출부에 sandbox 전달**

`crates/opengoose/src/cli/commands.rs`에서 `init_runtime(cli.port)` → `init_runtime(cli.port, cli.sandbox)`:

```rust
Some(Commands::Run { task }) => {
    let rt = crate::runtime::init_runtime(cli.port, cli.sandbox).await?;
    // ... 나머지 동일
}
None => {
    let log_rx = log_rx.expect("TUI mode must have log_rx");
    let rt = crate::runtime::init_runtime(cli.port, cli.sandbox).await?;
    // ... 나머지 동일
}
```

- [ ] **Step 3: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 4: 기존 테스트 통과 확인**

Run: `cargo test -p opengoose`
Expected: 전부 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/runtime.rs crates/opengoose/src/cli/commands.rs
git commit -m "feat(runtime): select SandboxValidationGate when --sandbox is set"
```

---

### Task 7: npm 프로젝트 지원 + 프로젝트 감지 로직

**Files:**
- Modify: `crates/opengoose/src/sandbox_gate.rs`

Cargo 프로젝트뿐 아니라 npm 프로젝트도 sandbox에서 검증할 수 있어야 한다. 또한 프로젝트 파일이 없으면 즉시 통과(기존 `post_execute` 동작과 일치).

- [ ] **Step 1: run_sandbox_validation에 프로젝트 감지 추가**

```rust
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
```

- [ ] **Step 2: 불필요한 import 정리**

`SandboxSession` import 추가:

```rust
#[cfg(target_os = "macos")]
use opengoose_sandbox::SandboxSession;
```

- [ ] **Step 3: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose/src/sandbox_gate.rs
git commit -m "feat(sandbox): npm project support + project type detection in SandboxValidationGate"
```

---

### Task 8: 통합 테스트 — SandboxValidationGate 단위 테스트

**Files:**
- Modify: `crates/opengoose/src/sandbox_gate.rs`

macOS에서만 실행되는 테스트. sandbox VM이 실제로 부팅/fork되므로 `#[ignore]` 태그를 붙여 CI에서 명시적으로만 실행한다.

- [ ] **Step 1: 테스트 작성**

`crates/opengoose/src/sandbox_gate.rs` 하단에 추가:

```rust
#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;
    use opengoose_rig::pipeline::{Middleware, PipelineContext};
    use opengoose_board::Board;
    use opengoose_board::work_item::{RigId, WorkItem};
    use opengoose_board::Priority;

    fn test_work_item() -> WorkItem {
        WorkItem {
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
        // Cargo.toml with no src/ → cargo check fails
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
```

- [ ] **Step 2: 테스트 컴파일 확인**

Run: `cargo test -p opengoose --no-run`
Expected: 성공

- [ ] **Step 3: macOS에서 ignored 테스트 실행**

Run: `cargo test -p opengoose sandbox_validation -- --ignored`
Expected: 3 tests PASS (macOS에서만)

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose/src/sandbox_gate.rs
git commit -m "test(sandbox): integration tests for SandboxValidationGate"
```

---

### Task 9: 로깅 + tracing 계측

**Files:**
- Modify: `crates/opengoose/src/sandbox_gate.rs`

sandbox validation에 tracing 스팬/이벤트를 추가하여 TUI Logs 탭에서 sandbox 상태를 확인할 수 있도록 한다.

- [ ] **Step 1: tracing import + 이벤트 추가**

`crates/opengoose/src/sandbox_gate.rs`에서:

```rust
#[cfg(target_os = "macos")]
use tracing::{info, warn, instrument};
```

`run_sandbox_validation`에 `#[instrument]` + 이벤트 추가:

```rust
#[cfg(target_os = "macos")]
#[instrument(skip(pool), fields(work_dir = %work_dir.display()))]
fn run_sandbox_validation(
    pool: &SandboxPool,
    work_dir: &std::path::Path,
) -> anyhow::Result<Option<String>> {
    let is_cargo = work_dir.join("Cargo.toml").exists();
    let is_npm = work_dir.join("package.json").exists();

    if !is_cargo && !is_npm {
        info!("no project files, skipping sandbox validation");
        return Ok(None);
    }

    info!("starting sandbox session");
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
        Err(e) => warn!(error = %e, "sandbox validation error"),
    }

    result
}
```

- [ ] **Step 2: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose/src/sandbox_gate.rs
git commit -m "feat(sandbox): add tracing instrumentation to SandboxValidationGate"
```

---

### Task 10: ARCHITECTURE.md 업데이트

**Files:**
- Modify: `docs/v0.2/ARCHITECTURE.md`

열린 질문 §14.4를 해결됨으로 마킹하고, sandbox-worker 통합 섹션을 추가한다.

- [ ] **Step 1: §14 열린 질문 4번 업데이트**

```markdown
4. ~~**샌드박스 추상화?**~~ **해결됨.** `opengoose-sandbox` 크레이트로 HVF microVM 구현 (§ 7.5). Worker 통합은 `SandboxValidationGate` 미들웨어로 구현됨 — `--sandbox` 플래그로 활성화.
```

- [ ] **Step 2: Blueprint 섹션(§ 2.4)에 sandbox 노트 추가**

기존 Blueprint 패턴 아래에:

```markdown
`--sandbox` 활성화 시, 결정론적 검증 노드(`cargo check`/`cargo test`)가 호스트 대신 microVM 안에서 실행된다. `SandboxValidationGate`가 `ValidationGate`를 대체하며, `SandboxPool`의 CoW fork로 sub-ms 레이턴시를 유지한다.
```

- [ ] **Step 3: Commit**

```bash
git add docs/v0.2/ARCHITECTURE.md
git commit -m "docs: update ARCHITECTURE.md with sandbox-worker integration"
```
