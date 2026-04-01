# Multi Worker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 복수의 Worker를 Web API(`/api/workers`)로 동적으로 추가/삭제하여 Board 작업을 병렬 처리한다.

**Architecture:** `WorkerPool`이 실행 중인 Worker들을 `RwLock<HashMap<String, WorkerHandle>>`로 관리. `Runtime.worker: Option<Arc<Worker>>`를 `Runtime.workers: Arc<WorkerPool>`로 교체. Web API 3개 엔드포인트(POST/GET/DELETE)로 런타임 중 Worker CRUD.

**Tech Stack:** Rust, axum, tokio, opengoose-rig (Worker/Rig), Goose Agent

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `crates/opengoose/src/worker_pool.rs` | WorkerPool — Worker 런타임 관리 | Create |
| `crates/opengoose/src/runtime.rs` | Runtime init — WorkerPool 생성 | Modify |
| `crates/opengoose/src/web/api/workers.rs` | Web API — /api/workers CRUD | Create |
| `crates/opengoose/src/web/api/mod.rs` | API 모듈 등록 | Modify |
| `crates/opengoose/src/web/mod.rs` | 라우터에 /api/workers 추가 | Modify |
| `crates/opengoose/src/cli/commands.rs` | dispatch에서 workers.cancel_all() 사용 | Modify |
| `crates/opengoose/src/cli/mod.rs` | --workers CLI 플래그 | Modify |
| `crates/opengoose/src/main.rs` | mod worker_pool 선언 | Modify |

---

### Task 1: WorkerPool 구조체

**Files:**
- Create: `crates/opengoose/src/worker_pool.rs`
- Modify: `crates/opengoose/src/main.rs`

WorkerPool은 실행 중인 Worker들을 관리하는 핵심 구조체.

- [ ] **Step 1: worker_pool.rs 파일 생성**

Create `crates/opengoose/src/worker_pool.rs`:

```rust
//! WorkerPool — 실행 중인 Worker 런타임 관리.
//! spawn/remove/list로 Worker를 동적으로 추가/삭제.

use anyhow::Result;
use opengoose_board::Board;
use opengoose_board::work_item::RigId;
use opengoose_rig::pipeline::Middleware;
use opengoose_rig::rig::Worker;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, warn};

struct WorkerHandle {
    worker: Arc<Worker>,
    join_handle: JoinHandle<()>,
}

/// Worker 생성 설정.
#[derive(Debug, Default)]
pub struct WorkerConfig {
    pub recipe: Option<String>,
    pub model: Option<String>,
}

/// Worker 상태 정보 (API 응답용).
#[derive(Debug, Serialize)]
pub struct WorkerInfo {
    pub id: String,
    pub status: &'static str,
}

pub struct WorkerPool {
    handles: RwLock<HashMap<String, WorkerHandle>>,
    board: Arc<Board>,
    middleware: Vec<Arc<dyn Middleware>>,
    counter: AtomicU64,
    sandbox: bool,
}

impl WorkerPool {
    pub fn new(board: Arc<Board>, middleware: Vec<Arc<dyn Middleware>>, sandbox: bool) -> Self {
        Self {
            handles: RwLock::new(HashMap::new()),
            board,
            middleware,
            counter: AtomicU64::new(1),
            sandbox,
        }
    }

    /// Worker 생성. id가 None이면 "worker-{n}" 자동 생성.
    pub async fn spawn(&self, id: Option<String>, config: WorkerConfig) -> Result<String> {
        let worker_id = id.unwrap_or_else(|| {
            let n = self.counter.fetch_add(1, Ordering::Relaxed);
            format!("worker-{n}")
        });

        // Check for duplicate
        if self.handles.read().await.contains_key(&worker_id) {
            anyhow::bail!("worker '{}' already exists", worker_id);
        }

        let agent = create_worker_agent_with_config(&config).await?;
        let rig_id = RigId::new(&worker_id);

        let worker = Arc::new(Worker::new(
            rig_id,
            Arc::clone(&self.board),
            agent,
            opengoose_rig::work_mode::TaskMode,
            self.middleware.clone(),
        ));

        let worker_handle = Arc::clone(&worker);
        let join_handle = tokio::spawn(async move { worker_handle.run().await });

        self.handles.write().await.insert(
            worker_id.clone(),
            WorkerHandle {
                worker,
                join_handle,
            },
        );

        info!(id = %worker_id, "worker spawned");
        Ok(worker_id)
    }

    /// Worker 즉시 종료. cancel + join.
    pub async fn remove(&self, id: &str) -> Result<()> {
        let handle = self
            .handles
            .write()
            .await
            .remove(id)
            .ok_or_else(|| anyhow::anyhow!("worker '{}' not found", id))?;

        handle.worker.cancel();
        // Wait for the pull loop to exit (bounded by cancel)
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            handle.join_handle,
        )
        .await;

        info!(id = %id, "worker removed");
        Ok(())
    }

    /// 실행 중인 Worker 목록.
    pub async fn list(&self) -> Vec<WorkerInfo> {
        self.handles
            .read()
            .await
            .keys()
            .map(|id| WorkerInfo {
                id: id.clone(),
                status: "running",
            })
            .collect()
    }

    /// 모든 Worker 종료.
    pub async fn cancel_all(&self) {
        let handles: Vec<_> = self.handles.write().await.drain().collect();
        for (id, handle) in handles {
            handle.worker.cancel();
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                handle.join_handle,
            )
            .await;
            info!(id = %id, "worker cancelled");
        }
    }

    /// Worker 수.
    pub async fn len(&self) -> usize {
        self.handles.read().await.len()
    }
}

/// Goose Agent 생성. config의 model이 있으면 환경 변수 대신 사용.
async fn create_worker_agent_with_config(config: &WorkerConfig) -> Result<goose::agents::Agent> {
    // Temporarily override GOOSE_MODEL if config specifies one
    let prev_model = config.model.as_ref().map(|m| {
        let prev = std::env::var("GOOSE_MODEL").ok();
        // SAFETY: single-threaded init path
        unsafe { std::env::set_var("GOOSE_MODEL", m) };
        prev
    });

    let result = crate::runtime::create_worker_agent().await;

    // Restore
    if let Some(prev) = prev_model {
        unsafe {
            match prev {
                Some(v) => std::env::set_var("GOOSE_MODEL", v),
                None => std::env::remove_var("GOOSE_MODEL"),
            }
        }
    }

    result.map(|(agent, _)| agent)
}
```

- [ ] **Step 2: main.rs에 mod 선언 추가**

`crates/opengoose/src/main.rs`에서 기존 mod 선언 블록에 추가:

```rust
mod worker_pool;
```

- [ ] **Step 3: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공 (dead_code 경고 가능 — 아직 사용 안 됨)

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose/src/worker_pool.rs crates/opengoose/src/main.rs
git commit -m "feat: add WorkerPool for dynamic worker management"
```

---

### Task 2: Runtime을 WorkerPool로 교체

**Files:**
- Modify: `crates/opengoose/src/runtime.rs`
- Modify: `crates/opengoose/src/cli/commands.rs`
- Modify: `crates/opengoose/src/cli/mod.rs`

Runtime.worker를 Runtime.workers로 교체하고 CLI에 --workers 플래그 추가.

- [ ] **Step 1: CLI에 --workers 플래그 추가**

`crates/opengoose/src/cli/mod.rs`의 `Cli` struct에 추가 (sandbox 뒤):

```rust
/// 초기 Worker 수
#[arg(long, default_value_t = 1, global = true)]
pub workers: u16,
```

- [ ] **Step 2: runtime.rs에서 Runtime 구조체 변경**

`crates/opengoose/src/runtime.rs` 수정:

```rust
use crate::worker_pool::WorkerPool;

pub struct Runtime {
    pub board: Arc<Board>,
    pub workers: Arc<WorkerPool>,
}
```

- [ ] **Step 3: init_runtime 시그니처와 구현 변경**

`init_runtime`을 수정하여 WorkerPool 사용:

```rust
pub async fn init_runtime(port: u16, sandbox: bool, num_workers: u16) -> Result<Runtime> {
    let board = Arc::new(Board::connect(&crate::db_url()).await?);
    web::spawn_server(Arc::clone(&board), port).await?;

    // Evolver
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(opengoose_evolver::run(Arc::clone(&board), stamp_notify));

    // Validation middleware
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

    // Worker pool
    let middleware: Vec<Arc<dyn Middleware>> = vec![
        Arc::new(ContextHydrator {
            skill_catalog: String::new(),
        }),
        validation,
    ];
    let workers = Arc::new(WorkerPool::new(
        Arc::clone(&board),
        middleware,
        sandbox,
    ));

    // Spawn initial workers
    for _ in 0..num_workers {
        if let Err(e) = workers.spawn(None, Default::default()).await {
            tracing::warn!(error = %e, "initial worker creation failed");
        }
    }

    Ok(Runtime { board, workers })
}
```

- [ ] **Step 4: cli/commands.rs의 호출부 수정**

`init_runtime` 호출을 3인자로 변경하고, `worker.cancel()`을 `workers.cancel_all()`로:

`Some(Commands::Run { task })` arm:
```rust
Some(Commands::Run { task }) => {
    let rt = crate::runtime::init_runtime(cli.port, cli.sandbox, cli.workers).await?;
    if rt.workers.len().await == 0 {
        anyhow::bail!("headless mode requires a worker; worker initialization failed");
    }
    let result = crate::headless::run_headless(&rt.board, &task).await;
    rt.workers.cancel_all().await;
    result
}
```

`None` arm (TUI mode):
```rust
None => {
    let log_rx = log_rx.expect("TUI mode must have log_rx");
    let rt = crate::runtime::init_runtime(cli.port, cli.sandbox, cli.workers).await?;
    let (agent, session_id) = crate::runtime::create_operator_agent().await?;
    let operator = Arc::new(opengoose_rig::rig::Operator::without_board(
        RigId::new("operator"),
        agent,
        &session_id,
    ));
    let result = crate::tui::run_tui(rt.board, operator, log_rx).await;
    rt.workers.cancel_all().await;
    result
}
```

- [ ] **Step 5: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 6: 기존 테스트 통과 확인**

Run: `cargo test -p opengoose`
Expected: 전부 PASS

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose/src/runtime.rs crates/opengoose/src/cli/commands.rs crates/opengoose/src/cli/mod.rs
git commit -m "feat: replace Runtime.worker with WorkerPool, add --workers flag"
```

---

### Task 3: Web API — /api/workers 엔드포인트

**Files:**
- Create: `crates/opengoose/src/web/api/workers.rs`
- Modify: `crates/opengoose/src/web/api/mod.rs`
- Modify: `crates/opengoose/src/web/mod.rs`

- [ ] **Step 1: AppState에 WorkerPool 추가**

`crates/opengoose/src/web/mod.rs`의 `AppState`에 workers 필드 추가:

```rust
use crate::worker_pool::WorkerPool;

#[derive(Clone)]
pub struct AppState {
    pub board: Arc<Board>,
    pub tx: broadcast::Sender<()>,
    pub workers: Arc<WorkerPool>,
}
```

`spawn_server` 시그니처에 workers 추가:

```rust
pub async fn spawn_server(
    board: Arc<Board>,
    port: u16,
    workers: Arc<WorkerPool>,
) -> anyhow::Result<()> {
```

`state` 생성부:

```rust
let state = AppState { board, tx, workers };
```

라우터에 workers 엔드포인트 추가:

```rust
.route("/api/workers", axum::routing::get(api::workers_list).post(api::workers_create))
.route("/api/workers/{id}", axum::routing::delete(api::workers_delete))
```

- [ ] **Step 2: runtime.rs에서 spawn_server 호출 수정**

```rust
web::spawn_server(Arc::clone(&board), port, Arc::clone(&workers)).await?;
```

주의: `workers`는 `spawn_server` 호출 시점에 이미 생성되어야 하므로, WorkerPool 생성을 web::spawn_server 호출 전으로 이동. 단, 초기 Worker spawn은 spawn_server 후에.

```rust
// Worker pool (create before web server so AppState can reference it)
let middleware: Vec<Arc<dyn Middleware>> = vec![...];
let workers = Arc::new(WorkerPool::new(Arc::clone(&board), middleware, sandbox));

// Web dashboard
web::spawn_server(Arc::clone(&board), port, Arc::clone(&workers)).await?;

// Spawn initial workers
for _ in 0..num_workers { ... }
```

- [ ] **Step 3: workers.rs API 핸들러 작성**

Create `crates/opengoose/src/web/api/workers.rs`:

```rust
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::worker_pool::{WorkerConfig, WorkerInfo};

#[derive(Deserialize)]
pub struct CreateWorkerRequest {
    pub id: Option<String>,
    pub recipe: Option<String>,
    pub model: Option<String>,
}

#[derive(Serialize)]
pub struct CreateWorkerResponse {
    pub id: String,
    pub status: &'static str,
}

pub async fn workers_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkerInfo>>, StatusCode> {
    Ok(Json(state.workers.list().await))
}

pub async fn workers_create(
    State(state): State<AppState>,
    Json(body): Json<CreateWorkerRequest>,
) -> Result<(StatusCode, Json<CreateWorkerResponse>), (StatusCode, String)> {
    let config = WorkerConfig {
        recipe: body.recipe,
        model: body.model,
    };
    match state.workers.spawn(body.id, config).await {
        Ok(id) => Ok((
            StatusCode::CREATED,
            Json(CreateWorkerResponse {
                id,
                status: "running",
            }),
        )),
        Err(e) => Err((StatusCode::BAD_REQUEST, format!("Failed to create worker: {e}"))),
    }
}

pub async fn workers_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CreateWorkerResponse>, (StatusCode, String)> {
    match state.workers.remove(&id).await {
        Ok(()) => Ok(Json(CreateWorkerResponse {
            id,
            status: "stopped",
        })),
        Err(e) => Err((StatusCode::NOT_FOUND, format!("Failed to remove worker: {e}"))),
    }
}
```

- [ ] **Step 4: api/mod.rs에 workers 모듈 등록**

`crates/opengoose/src/web/api/mod.rs`:

```rust
mod board;
mod rigs;
mod skills;
mod workers;

pub use board::{board_claim, board_create, board_get, board_list};
pub use rigs::{rig_detail, rigs_list};
pub use skills::{skill_delete, skill_detail, skill_promote, skills_list};
pub use workers::{workers_create, workers_delete, workers_list};

use super::AppState;
```

- [ ] **Step 5: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose/src/web/api/workers.rs crates/opengoose/src/web/api/mod.rs crates/opengoose/src/web/mod.rs crates/opengoose/src/runtime.rs
git commit -m "feat(web): add /api/workers endpoints for dynamic worker management"
```

---

### Task 4: Web API 테스트

**Files:**
- Modify: `crates/opengoose/src/web/api/workers.rs`

- [ ] **Step 1: workers.rs에 테스트 모듈 추가**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::AppState;
    use crate::worker_pool::WorkerPool;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use http_body_util::BodyExt;
    use opengoose_board::Board;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    async fn test_app() -> Router {
        let board = Arc::new(Board::in_memory().await.expect("board"));
        let (tx, _) = broadcast::channel::<()>(64);
        let pool = Arc::new(WorkerPool::new(board.clone(), vec![], false));
        let state = AppState {
            board,
            tx,
            workers: pool,
        };
        Router::new()
            .route("/api/workers", axum::routing::get(workers_list).post(workers_create))
            .route("/api/workers/{id}", axum::routing::delete(workers_delete))
            .with_state(state)
    }

    #[tokio::test]
    async fn list_workers_empty() {
        let app = test_app().await;
        let resp = app
            .oneshot(Request::get("/api/workers").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let workers: Vec<WorkerInfo> = serde_json::from_slice(&body).unwrap();
        assert!(workers.is_empty());
    }

    #[tokio::test]
    async fn delete_nonexistent_worker_returns_404() {
        let app = test_app().await;
        let resp = app
            .oneshot(
                Request::delete("/api/workers/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
```

Note: `workers_create` 테스트는 실제 Goose Agent 생성이 필요하므로 (provider 설정 필요), 여기서는 list와 delete만 테스트. create는 통합 테스트 레벨에서 커버.

- [ ] **Step 2: cargo test 통과 확인**

Run: `cargo test -p opengoose list_workers_empty delete_nonexistent`
Expected: 2 tests PASS

- [ ] **Step 3: 전체 테스트 확인**

Run: `cargo test`
Expected: 전부 PASS

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose/src/web/api/workers.rs
git commit -m "test(web): add /api/workers endpoint tests"
```

---

### Task 5: WorkerPool 단위 테스트

**Files:**
- Modify: `crates/opengoose/src/worker_pool.rs`

- [ ] **Step 1: worker_pool.rs에 테스트 모듈 추가**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_board::Board;

    #[tokio::test]
    async fn pool_starts_empty() {
        let board = Arc::new(Board::in_memory().await.expect("board"));
        let pool = WorkerPool::new(board, vec![], false);
        assert_eq!(pool.len().await, 0);
        assert!(pool.list().await.is_empty());
    }

    #[tokio::test]
    async fn remove_nonexistent_returns_error() {
        let board = Arc::new(Board::in_memory().await.expect("board"));
        let pool = WorkerPool::new(board, vec![], false);
        let result = pool.remove("ghost").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cancel_all_on_empty_pool_is_noop() {
        let board = Arc::new(Board::in_memory().await.expect("board"));
        let pool = WorkerPool::new(board, vec![], false);
        pool.cancel_all().await; // should not panic
        assert_eq!(pool.len().await, 0);
    }
}
```

Note: spawn 테스트는 Goose Agent 생성이 필요하므로 단위 테스트에서는 제외. 위 테스트는 pool 자체의 동작만 검증.

- [ ] **Step 2: 테스트 통과 확인**

Run: `cargo test -p opengoose pool_starts_empty remove_nonexistent cancel_all_on_empty`
Expected: 3 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose/src/worker_pool.rs
git commit -m "test: add WorkerPool unit tests"
```

---

### Task 6: 기존 web 테스트 수정 + clippy/fmt

**Files:**
- Modify: `crates/opengoose/src/web/api/board.rs` (테스트의 AppState에 workers 추가)
- Modify: `crates/opengoose/src/web/api/rigs.rs` (테스트의 AppState에 workers 추가)

AppState에 `workers` 필드가 추가되었으므로, 기존 web API 테스트에서 AppState를 생성하는 부분에도 `workers` 필드를 추가해야 한다.

- [ ] **Step 1: 기존 테스트의 AppState에 workers 추가**

`board.rs`와 `rigs.rs`의 테스트에서 `AppState { board, tx }` → `AppState { board, tx, workers: Arc::new(WorkerPool::new(board.clone(), vec![], false)) }`.

각 테스트 파일의 `test_app()` 또는 `AppState` 생성부를 검색하여 수정.

- [ ] **Step 2: cargo check --all-targets + cargo test 통과 확인**

Run: `cargo check --all-targets && cargo test`
Expected: 전부 통과

- [ ] **Step 3: cargo clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: clean

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "fix: update existing web tests for WorkerPool in AppState"
```

---

### Task 7: ARCHITECTURE.md 업데이트

**Files:**
- Modify: `docs/v0.2/ARCHITECTURE.md`

- [ ] **Step 1: §14 열린 질문 5번 업데이트**

기존:
```
5. **멀티 Worker CLI UX?** 현재 단일 Worker. 복수 Worker 시 동시 스트림 표시 전략 미정.
```

변경:
```
5. ~~**멀티 Worker CLI UX?**~~ **해결됨.** `WorkerPool`로 동적 Worker 관리. Web API(`/api/workers`)로 추가/삭제. `--workers N`으로 초기 수 지정.
```

- [ ] **Step 2: §11 Runtime 와이어링에 WorkerPool 반영**

Runtime 코드 예시를 WorkerPool 사용으로 업데이트.

- [ ] **Step 3: Commit**

```bash
git add docs/v0.2/ARCHITECTURE.md
git commit -m "docs: update ARCHITECTURE.md with multi-worker support"
```
