# Multi Worker Design

> **작성일:** 2026-03-31
> **상태:** 승인됨

## 목표

복수의 Worker를 런타임 중 동적으로 추가/삭제할 수 있게 하여, 여러 작업을 병렬로 처리한다.

## 결정 사항

| 항목 | 결정 |
|------|------|
| Worker 수 | 동적 — 런타임 중 추가/삭제 |
| 인터페이스 | Web API (`/api/workers`) |
| Worker별 설정 | recipe/model 지정 가능, 기본값 있음 |
| 삭제 동작 | 즉시 취소 (CancellationToken + unclaim) |
| 기존 /api/rigs | 분리 유지 — Board 메타데이터만 담당 |

## WorkerPool

`Runtime`에 `WorkerPool` 추가. 실행 중인 Worker들을 관리.

```rust
pub struct WorkerPool {
    handles: RwLock<HashMap<String, WorkerHandle>>,
    board: Arc<Board>,
    middleware: Vec<Arc<dyn Middleware>>,
}

pub struct WorkerHandle {
    pub id: String,
    pub worker: Arc<Worker>,
    pub join_handle: JoinHandle<()>,
}
```

### API

```rust
impl WorkerPool {
    pub fn new(board: Arc<Board>, middleware: Vec<Arc<dyn Middleware>>) -> Self;

    /// Worker 생성. id가 None이면 "worker-{n}" 자동 생성.
    pub async fn spawn(&self, id: Option<String>, config: WorkerConfig) -> Result<String>;

    /// Worker 즉시 종료. cancel + unclaim + join.
    pub async fn remove(&self, id: &str) -> Result<()>;

    /// 실행 중인 Worker 목록.
    pub async fn list(&self) -> Vec<WorkerInfo>;

    /// 모든 Worker 종료.
    pub async fn cancel_all(&self);
}

pub struct WorkerConfig {
    pub recipe: Option<String>,
    pub model: Option<String>,
}

pub struct WorkerInfo {
    pub id: String,
    pub status: WorkerStatus,
    pub current_item: Option<WorkItemSummary>,
}
```

## Worker 생성 흐름

```
POST /api/workers { "id": "worker-2", "recipe": "small", "model": "claude-sonnet" }
  → WorkerPool.spawn(id, config)
    → create_worker_agent(config)
    → Worker::new(id, board, agent, TaskMode, middleware)
    → tokio::spawn(worker.run())
    → handles.insert(id, WorkerHandle)
```

id는 optional — 없으면 `worker-{counter}` 자동 생성.
recipe와 model도 optional — 없으면 환경 변수 기본값.

## Worker 삭제 흐름

```
DELETE /api/workers/worker-2
  → WorkerPool.remove("worker-2")
    → worker.cancel()
    → join_handle.await
    → handles.remove(id)
```

Worker의 `run()` loop이 cancel을 감지하면 현재 진행 중인 작업을 unclaim하고 종료.
Worker가 claim 후 cancel되면 Worker의 기존 `process_claimed_item` 흐름에서 `cancel.is_cancelled()` 체크 → unclaim 자동 수행.

## Web API

```
POST   /api/workers         — Worker 생성
GET    /api/workers          — Worker 목록 (id, 상태, 현재 작업)
DELETE /api/workers/:id      — Worker 즉시 종료
```

### 요청/응답

```json
// POST /api/workers
// Request:
{ "id": "worker-2", "recipe": "small", "model": "claude-sonnet" }
// Response (201):
{ "id": "worker-2", "status": "running" }

// GET /api/workers
// Response (200):
[
  { "id": "worker-1", "status": "running", "current_item": null },
  { "id": "worker-2", "status": "running", "current_item": { "id": 5, "title": "auth 리팩토링" } }
]

// DELETE /api/workers/worker-2
// Response (200):
{ "id": "worker-2", "status": "stopped" }
```

`current_item`은 Board의 `claimed_by(rig_id)` 쿼리로 조회.

## Runtime 변경

### 기존

```rust
pub struct Runtime {
    pub board: Arc<Board>,
    pub worker: Option<Arc<Worker>>,
}
```

### 변경 후

```rust
pub struct Runtime {
    pub board: Arc<Board>,
    pub workers: Arc<WorkerPool>,
}
```

### init_runtime 변경

- `WorkerPool::new(board, middleware)` 생성
- 초기 Worker 1개 spawn (기존 동작 유지)
- `--workers N` 플래그로 초기 수 조절 가능

### CLI 호출부 변경

- `commands.rs`의 `worker.cancel()` → `workers.cancel_all()`
- headless 모드에서도 `workers.cancel_all()` 사용

## Agent 생성

`create_worker_agent()`를 `WorkerConfig`를 받도록 확장:

```rust
pub async fn create_worker_agent_with_config(
    config: &WorkerConfig,
) -> Result<(Agent, String)>
```

- `config.recipe` 있으면 Goose recipe로 사용
- `config.model` 있으면 `GOOSE_MODEL` 환경 변수 대신 사용
- 둘 다 None이면 기존 `create_worker_agent()`와 동일

## Scope 밖

- TUI에서 Worker 상태 표시 (나중에)
- Worker별 tag 매칭 (나중에)
- SSE Worker 상태 실시간 스트리밍 (나중에)
- Worker auto-scaling (나중에)
- Worker 재시작 / health check (나중에)
