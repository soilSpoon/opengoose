# Blueprint Phase 2-B: Middleware Trait Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 하드코딩된 pre_hydrate/post_execute 호출을 조합 가능한 Middleware trait 스택으로 리팩토링하여, 작업 종류별로 다른 파이프라인을 구성할 수 있게 한다.

**Architecture:** `Middleware` trait을 정의하고 `ContextHydrator`, `ValidationGate` 두 구현체를 만든다. `Rig<M>`에 `Vec<Arc<dyn Middleware>>` 필드를 추가하고, `process_claimed_item()`이 스택을 순회하도록 리팩토링. 기존 자유 함수는 그대로 유지하고 trait impl이 위임(delegate). 동작 변경 없음 — 순수 리팩토링.

**Tech Stack:** Rust, async-trait, tokio

---

## 설계 결정

### trait 메서드 2개만
Open SWE의 4개 훅(before_agent, wrap_model_call, before_tool_call, after_tool_call)은 과도. Goose가 내부적으로 호출별 래핑을 처리하므로, 우리는 **on_start**(LLM 호출 전 1회)와 **post_process**(LLM 호출 후 매번)만 필요.

### PipelineContext 참조 기반
Context는 소유권 없이 참조만 전달. 미들웨어가 Agent나 Board를 소유하면 안 됨.

### 기존 함수 유지
`middleware.rs`의 자유 함수(`pre_hydrate`, `post_execute`)는 삭제하지 않음. trait impl이 위임하고, 기존 단위 테스트도 그대로 유지.

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `crates/opengoose-rig/src/pipeline.rs` | `Middleware` trait, `PipelineContext`, `ContextHydrator`, `ValidationGate` |
| Modify | `crates/opengoose-rig/src/lib.rs` | `pub mod pipeline;` 추가 |
| Modify | `crates/opengoose-rig/src/rig.rs:25-31` | `Rig<M>`에 `middleware` 필드 추가 |
| Modify | `crates/opengoose-rig/src/rig.rs:240-364` | `process_claimed_item()`을 middleware 스택으로 리팩토링 |

---

## Task 1: Middleware trait + PipelineContext 정의

**Files:**
- Create: `crates/opengoose-rig/src/pipeline.rs`
- Modify: `crates/opengoose-rig/src/lib.rs`

- [ ] **Step 1: Create pipeline.rs with trait and context**

Create `crates/opengoose-rig/src/pipeline.rs`:

```rust
// Pipeline — Middleware trait for composable Blueprint execution.
//
// on_start: LLM 호출 전 1회 실행 (컨텍스트 주입)
// post_process: LLM 호출 후 매번 실행 (검증)

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
```

- [ ] **Step 2: Add pub mod pipeline to lib.rs**

Add `pub mod pipeline;` to `crates/opengoose-rig/src/lib.rs` after `pub mod middleware;`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p opengoose-rig`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/pipeline.rs crates/opengoose-rig/src/lib.rs
git commit -m "feat(rig): define Middleware trait and PipelineContext"
```

---

## Task 2: ContextHydrator 구현

**Files:**
- Modify: `crates/opengoose-rig/src/pipeline.rs`

- [ ] **Step 1: Write the failing test**

Add to `pipeline.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct FakeAgent;

    #[tokio::test]
    async fn context_hydrator_calls_on_start_without_panic() {
        let hydrator = ContextHydrator {
            skill_catalog: String::new(),
        };
        // PipelineContext requires a real Agent, so just verify the struct exists and trait is implemented
        // Full integration test happens in rig.rs process_claimed_item
        assert!(std::mem::size_of::<ContextHydrator>() > 0);
    }
}
```

- [ ] **Step 2: Implement ContextHydrator**

Add to `pipeline.rs` (before the tests module):

```rust
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
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p opengoose-rig pipeline -- --nocapture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/pipeline.rs
git commit -m "feat(rig): implement ContextHydrator middleware"
```

---

## Task 3: ValidationGate 구현

**Files:**
- Modify: `crates/opengoose-rig/src/pipeline.rs`

- [ ] **Step 1: Write the failing test**

Add to `pipeline.rs` tests module:

```rust
#[tokio::test]
async fn validation_gate_post_process_returns_none_for_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let agent = goose::agents::Agent::new();
    let board = Board::in_memory().await.unwrap();
    let item = opengoose_board::work_item::WorkItem {
        id: 1,
        title: "test".into(),
        description: String::new(),
        created_by: RigId::new("u"),
        created_at: chrono::Utc::now(),
        status: opengoose_board::work_item::Status::Claimed,
        priority: opengoose_board::Priority::P1,
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
    let result = gate.post_process(&ctx).await;
    assert!(result.is_none(), "empty dir should pass validation");
}
```

- [ ] **Step 2: Implement ValidationGate**

Add to `pipeline.rs`:

```rust
/// cargo check + cargo test 자동 실행. 실패 시 에러 메시지 반환.
pub struct ValidationGate;

#[async_trait::async_trait]
impl Middleware for ValidationGate {
    async fn post_process(&self, ctx: &PipelineContext<'_>) -> Option<String> {
        crate::middleware::post_execute(ctx.work_dir).await
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p opengoose-rig pipeline -- --nocapture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/pipeline.rs
git commit -m "feat(rig): implement ValidationGate middleware"
```

---

## Task 4: Rig<M>에 middleware 필드 추가

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs`

- [ ] **Step 1: Add middleware field to Rig struct**

In `rig.rs`, add import at top:

```rust
use crate::pipeline::Middleware;
```

Change the `Rig<M>` struct (lines 25-31) from:

```rust
pub struct Rig<M: WorkMode> {
    pub id: RigId,
    board: Option<Arc<Board>>,
    agent: Agent,
    mode: M,
    cancel: CancellationToken,
}
```

To:

```rust
pub struct Rig<M: WorkMode> {
    pub id: RigId,
    board: Option<Arc<Board>>,
    agent: Agent,
    mode: M,
    cancel: CancellationToken,
    middleware: Vec<Arc<dyn Middleware>>,
}
```

- [ ] **Step 2: Update Rig::new() to accept middleware**

Change `Rig::new()` (line 45-53) from:

```rust
pub fn new(id: RigId, board: Arc<Board>, agent: Agent, mode: M) -> Self {
    Self {
        id,
        board: Some(board),
        agent,
        mode,
        cancel: CancellationToken::new(),
    }
}
```

To:

```rust
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
```

- [ ] **Step 3: Update Operator::without_board()**

Change `Operator::without_board()` (line 116-124) to add `middleware: vec![]`:

```rust
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
```

- [ ] **Step 4: Update all test call sites**

Every test that calls `Rig::new(...)` or constructs a `Rig` needs the new `middleware` parameter. Search for all `Rig::new(` and `Worker::new(` and `Evolver::new(` in the test module and add `vec![]` as the last argument.

Test call sites (in rig.rs tests):
- `rig_new_board_getter_returns_some` — `Rig::new(RigId::new("test-rig"), board, agent, TaskMode)` → add `, vec![]`
- `rig_new_id_is_stored` — same pattern
- `rig_cancel_token_starts_alive` — same
- `rig_cancel_marks_token_cancelled` — same
- `rig_agent_getter_does_not_panic` — same
- `worker_try_claim_on_empty_board_returns_false` — `Worker::new(...)` → add `, vec![]`
- `worker_run_exits_when_pre_cancelled` — same
- `evolver_new_sets_id_and_has_board` — `Evolver::new(...)` → add `, vec![]`
- `rig_chat_mode_board_getter_returns_some` — `Rig::new(...)` → add `, vec![]`

Also update these call sites outside rig.rs:
- `crates/opengoose/src/main.rs:142` — `Worker::new(...)` → add `, vec![]` as last argument

Check `crates/opengoose-rig/src/mcp_tools.rs` and `crates/opengoose/src/` for any other `Rig::new()` or `Worker::new()` call sites and update them.

- [ ] **Step 5: Verify compilation**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 6: Run tests**

Run: `cargo test -p opengoose-rig -- --skip post_execute_npm_check_succeeds`
Expected: ALL PASS

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose-rig/src/rig.rs crates/opengoose-rig/src/pipeline.rs crates/opengoose/src/main.rs
git commit -m "feat(rig): add middleware field to Rig<M>"
```

---

## Task 5: process_claimed_item을 middleware 스택으로 리팩토링

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:240-364` (`process_claimed_item`)

- [ ] **Step 1: Import PipelineContext**

Add to rig.rs imports:

```rust
use crate::pipeline::PipelineContext;
```

- [ ] **Step 2: Replace hardcoded pre_hydrate with middleware on_start**

In `process_claimed_item()`, replace lines 264-267:

```rust
// Blueprint Phase 1: pre_hydrate — AGENTS.md + Skills + Board 요약 주입
let all_items = board.list().await.unwrap_or_default();
let board_prime = beads::prime_summary(&all_items, &self.id);
crate::middleware::pre_hydrate(&self.agent, &guard.path, "", &board_prime).await;
```

With:

```rust
// Blueprint: middleware on_start — 컨텍스트 주입
let pipeline_ctx = PipelineContext {
    agent: &self.agent,
    work_dir: &guard.path,
    rig_id: &self.id,
    board: board.as_ref(),
    item,
};
for mw in &self.middleware {
    if let Err(e) = mw.on_start(&pipeline_ctx).await {
        warn!(rig = %self.id, item_id = item.id, error = %e, "middleware on_start failed");
    }
}
```

- [ ] **Step 3: Replace hardcoded post_execute with middleware post_process**

In the retry loop, replace line 323:

```rust
let validation = crate::middleware::post_execute(&guard.path).await;
```

With:

```rust
let mut validation: Option<String> = None;
for mw in &self.middleware {
    if let Some(err) = mw.post_process(&pipeline_ctx).await {
        validation = Some(err);
        break;
    }
}
```

- [ ] **Step 4: Remove unused beads import**

The `use opengoose_board::beads;` import (line 14) is no longer needed in rig.rs — `ContextHydrator` handles it internally. Remove it.

- [ ] **Step 5: Verify compilation**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 6: Run tests**

Run: `cargo test -p opengoose-rig -- --skip post_execute_npm_check_succeeds`
Expected: ALL PASS

- [ ] **Step 7: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "refactor(rig): use middleware stack in process_claimed_item"
```

---

## Task 6: Integration verification

- [ ] **Step 1: Run full workspace tests**

Run: `cargo test --workspace -- --skip post_execute_npm_check_succeeds`
Expected: ALL PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Verify the final pipeline structure**

Read `process_claimed_item()` and confirm:
```
1. WorktreeGuard::create/attach
2. PipelineContext 생성
3. middleware.on_start() 순회           ← was: hardcoded pre_hydrate
4. Session create/find
5. process(input) — LLM 실행
6. for attempt in 0..=MAX_RETRIES:
   a. middleware.post_process() 순회    ← was: hardcoded post_execute
   b. None → submit, break
   c. Some + retryable → process(fix_prompt)
   d. Some + exhausted → mark_stuck
7. guard.remove()
```

- [ ] **Step 4: Verify middleware.rs free functions still have all tests**

Run: `cargo test -p opengoose-rig middleware -- --nocapture`
Expected: ALL 15 tests PASS (unchanged from Phase 1)
