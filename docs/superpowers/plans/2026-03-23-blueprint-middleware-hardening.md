# Blueprint Middleware Hardening Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix silent error swallowing, rename for clarity, wire middleware in production, and remove dead tests — making the middleware stack production-ready.

**Architecture:** Five tasks across 3 files (`pipeline.rs`, `rig.rs`, `main.rs`). Tasks 1-3 modify `opengoose-rig`, Task 4 wires production, Task 5 refactors. Task 5 **supersedes** the manual loops written in Task 2 with helper methods — the loops are intermediate scaffolding. No new files created.

**Note on scope:** The session-creation failure path (rig.rs lines 307-314) already has the correct `abandon → guard.remove → return` pattern and is out of scope for this plan.

**Tech Stack:** Rust, async-trait, anyhow, tokio

---

## File Map

| File | Changes |
|------|---------|
| `crates/opengoose-rig/src/pipeline.rs` | Rename `post_process` → `validate`, change return type to `Result<Option<String>>`, propagate `board.list()` error, remove useless test, add `PipelineContext::run_on_start()` / `run_validate()` |
| `crates/opengoose-rig/src/rig.rs` | Update trait method calls, abort on `on_start` failure, handle `validate` `Result` |
| `crates/opengoose/src/main.rs` | Wire `ContextHydrator` + `ValidationGate` into Worker |

---

### Task 1: Rename `post_process` → `validate`, change return type, propagate infra errors

**Files:**
- Modify: `crates/opengoose-rig/src/pipeline.rs:1-4,20-34,56-61,78-105` (trait + impl + test)
- Modify: `crates/opengoose-rig/src/middleware.rs:32-42,54-102,184-241` (post_execute + helpers + tests)

**Why:** `post_process` is vague — the only post-processing is validation. `Option<String>` conflates "validation passed" with "cargo binary not found" (both → `None`). The `.ok()?` in `run_check` silently treats a missing `cargo` as a pass. All fixed in one atomic commit.

- [ ] **Step 1: Update trait definition in `pipeline.rs`**

Change the file header comment (line 4) from `post_process` to `validate`.

Change the trait doc comment (line 23) to:
```
/// validate: LLM 호출 후 매번. Ok(None) = 통과, Ok(Some) = 검증 실패, Err = 인프라 실패.
```

Change the trait method from:
```rust
async fn post_process(&self, ctx: &PipelineContext<'_>) -> Option<String> {
    let _ = ctx;
    None
}
```
to:
```rust
/// 검증 실행. Ok(None) = 통과, Ok(Some(msg)) = 검증 실패, Err = 인프라 실패.
async fn validate(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<Option<String>> {
    let _ = ctx;
    Ok(None)
}
```

- [ ] **Step 2: Update ValidationGate impl in `pipeline.rs`**

```rust
#[async_trait::async_trait]
impl Middleware for ValidationGate {
    async fn validate(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<Option<String>> {
        crate::middleware::post_execute(ctx.work_dir).await
    }
}
```

- [ ] **Step 3: Change `post_execute`, `run_check`, `run_npm_check` in `middleware.rs`**

All three functions change return type from `Option<String>` to `anyhow::Result<Option<String>>`. Replace `.ok()?` with `?` to propagate command execution failures:

```rust
pub async fn post_execute(work_dir: &Path) -> anyhow::Result<Option<String>> {
    if work_dir.join("Cargo.toml").exists() {
        return run_check(work_dir).await;
    }
    if work_dir.join("package.json").exists() {
        return run_npm_check(work_dir).await;
    }
    Ok(None)
}

async fn run_check(work_dir: &Path) -> anyhow::Result<Option<String>> {
    let check_output = tokio::process::Command::new("cargo")
        .arg("check")
        .arg("--message-format=short")
        .current_dir(work_dir)
        .output()
        .await?;

    if !check_output.status.success() {
        let stderr = String::from_utf8_lossy(&check_output.stderr);
        return Ok(Some(format!("cargo check failed:\n{stderr}")));
    }

    let test_output = tokio::process::Command::new("cargo")
        .arg("test")
        .current_dir(work_dir)
        .output()
        .await?;

    if !test_output.status.success() {
        let stderr = String::from_utf8_lossy(&test_output.stderr);
        let stdout = String::from_utf8_lossy(&test_output.stdout);
        return Ok(Some(format!("cargo test failed:\n{stdout}\n{stderr}")));
    }

    Ok(None)
}

async fn run_npm_check(work_dir: &Path) -> anyhow::Result<Option<String>> {
    let output = tokio::process::Command::new("npm")
        .arg("test")
        .arg("--")
        .arg("--passWithNoTests")
        .current_dir(work_dir)
        .output()
        .await?;

    if output.status.success() {
        Ok(None)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(Some(format!("npm test failed:\n{stderr}")))
    }
}
```

- [ ] **Step 4: Update existing tests in `middleware.rs`**

All `post_execute` tests need `.unwrap()` added since it now returns `Result`:

```rust
// post_execute_returns_none_when_no_project_files
let result = post_execute(tmp.path()).await.unwrap();
assert!(result.is_none());

// post_execute_runs_cargo_check_when_cargo_toml_present
let result = post_execute(tmp.path()).await.unwrap();
assert!(result.is_some());
assert!(result.unwrap().contains("cargo check failed"));

// etc. for ALL post_execute_* tests — add .unwrap() to every call
```

- [ ] **Step 5: Add infra failure test in `middleware.rs`**

This test verifies the whole point of the change — a missing `cargo` binary now returns `Err`, not `None`:

```rust
#[tokio::test]
async fn post_execute_returns_err_when_cargo_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();

    // Use a nonexistent directory as PATH so cargo/npm can't be found
    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    unsafe { std::env::set_var("PATH", "/nonexistent-dir-for-test"); }

    let result = post_execute(tmp.path()).await;

    unsafe { std::env::set_var("PATH", &orig_path); }
    assert!(result.is_err(), "missing cargo should return Err, not Ok(None)");
}
```

- [ ] **Step 6: Update test in `pipeline.rs`**

Rename `validation_gate_post_process_returns_none_for_empty_dir` → `validation_gate_returns_ok_none_for_empty_dir`:
```rust
let result = gate.validate(&ctx).await.unwrap();
assert!(result.is_none(), "empty dir should pass validation");
```

- [ ] **Step 7: Verify**

Run: `cargo test -p opengoose-rig -- --skip post_execute_npm_check`
Expected: All tests pass (including the new infra failure test).

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose-rig/src/pipeline.rs crates/opengoose-rig/src/middleware.rs
git commit -m "fix(rig): propagate infra errors in validate, rename post_process → validate"
```

---

### Task 2: Propagate `board.list()` error in ContextHydrator

**Files:**
- Modify: `crates/opengoose-rig/src/pipeline.rs:44-50` (ContextHydrator::on_start)

**Why:** `unwrap_or_default()` silently swallows SQLite errors. The LLM would run without board context, wasting tokens on uninformed work.

- [ ] **Step 1: Replace `unwrap_or_default` with `?`**

```rust
#[async_trait::async_trait]
impl Middleware for ContextHydrator {
    async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> {
        let all_items = ctx.board.list().await
            .map_err(|e| anyhow::anyhow!("board.list() failed: {e}"))?;
        let board_prime = opengoose_board::beads::prime_summary(&all_items, ctx.rig_id);
        crate::middleware::pre_hydrate(ctx.agent, ctx.work_dir, &self.skill_catalog, &board_prime)
            .await;
        Ok(())
    }
}
```

- [ ] **Step 2: Verify**

Run: `cargo check -p opengoose-rig`
Expected: Clean compile.

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose-rig/src/pipeline.rs
git commit -m "fix(rig): propagate board.list() error in ContextHydrator"
```

---

### Task 3: Abort on `on_start` failure + handle `validate` Result in rig.rs

**Note:** The manual loops written here are **intermediate scaffolding**. Task 5 will replace them with `PipelineContext` helper methods. This task exists to get the error handling semantics right first.

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:273-285` (on_start loop)
- Modify: `crates/opengoose-rig/src/rig.rs:340-347` (validate loop)

**Why:** Currently `on_start` failure is logged but execution continues — the LLM runs without context, wasting API calls. And `validate` now returns `Result` which must be handled.

- [ ] **Step 1: Abort on `on_start` failure**

Replace lines 281-285:
```rust
for mw in &self.middleware {
    if let Err(e) = mw.on_start(&pipeline_ctx).await {
        warn!(rig = %self.id, item_id = item.id, error = %e, "middleware on_start failed, abandoning");
        board.abandon(item.id).await.ok();
        guard.remove().await;
        return;
    }
}
```

- [ ] **Step 2: Handle `validate` Result**

Replace lines 341-347:
```rust
let mut validation: Option<String> = None;
for mw in &self.middleware {
    match mw.validate(&pipeline_ctx).await {
        Ok(Some(err)) => {
            validation = Some(err);
            break;
        }
        Ok(None) => {}
        Err(e) => {
            warn!(rig = %self.id, item_id = item.id, error = %e, "validation infra failed, abandoning");
            board.abandon(item.id).await.ok();
            guard.remove().await;
            return;
        }
    }
}
```

- [ ] **Step 3: Verify**

Run: `cargo check --workspace`
Expected: Clean compile.

Run: `cargo test -p opengoose-rig -- --skip post_execute_npm_check`
Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "fix(rig): abort on middleware on_start failure, handle validate Result"
```

---

### Task 4: Wire ContextHydrator + ValidationGate in main.rs

**Files:**
- Modify: `crates/opengoose/src/main.rs:142-148` (Worker::new call)

**Why:** This is the most critical fix. The middleware stack is currently `vec![]` in production — none of the middleware runs. The entire Phase 2-B refactoring is dead code.

- [ ] **Step 1: Add imports**

At the top of `main.rs`, add:
```rust
use opengoose_rig::pipeline::{ContextHydrator, ValidationGate};
```

- [ ] **Step 2: Wire middleware in `init_runtime`**

Replace the Worker construction (lines 142-148):
```rust
let worker = Arc::new(opengoose_rig::rig::Worker::new(
    RigId::new("worker"),
    Arc::clone(&board),
    worker_agent,
    opengoose_rig::work_mode::TaskMode,
    vec![
        Arc::new(ContextHydrator { skill_catalog: String::new() }),
        Arc::new(ValidationGate),
    ],
));
```

Note: `skill_catalog` is empty for now — skill loading will populate it later when the skill system is wired.

- [ ] **Step 3: Add `use std::sync::Arc` import if not already present** (it is — line 20)

- [ ] **Step 4: Verify**

Run: `cargo check --workspace`
Expected: Clean compile.

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/main.rs
git commit -m "feat(rig): wire ContextHydrator + ValidationGate in Worker"
```

---

### Task 5: Remove useless test + add PipelineContext helper methods

**Note:** This task **supersedes** the manual loops written in Task 3. The `run_on_start` and `run_validate` helpers replace the `for mw in &self.middleware` loops entirely — do not keep both.

**Files:**
- Modify: `crates/opengoose-rig/src/pipeline.rs:68-76` (remove test)
- Modify: `crates/opengoose-rig/src/pipeline.rs:11-18` (add methods)

**Why:** `context_hydrator_exists_and_is_middleware` test asserts `size_of_val > 0` which tests nothing. And the middleware iteration in `rig.rs` can be simplified with helper methods on `PipelineContext`.

- [ ] **Step 1: Remove the useless test**

Delete the `context_hydrator_exists_and_is_middleware` test entirely (lines 68-76).

- [ ] **Step 2: Add helper methods to PipelineContext**

After the `PipelineContext` struct definition, add:
```rust
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
```

This requires adding `use std::sync::Arc;` to the imports in pipeline.rs.

- [ ] **Step 3: Simplify rig.rs to use helpers**

Replace the on_start loop in `process_claimed_item`:
```rust
// Blueprint: middleware on_start — 컨텍스트 주입
let pipeline_ctx = PipelineContext {
    agent: &self.agent,
    work_dir: &guard.path,
    rig_id: &self.id,
    board: board.as_ref(),
    item,
};
if let Err(e) = pipeline_ctx.run_on_start(&self.middleware).await {
    warn!(rig = %self.id, item_id = item.id, error = %e, "middleware on_start failed, abandoning");
    board.abandon(item.id).await.ok();
    guard.remove().await;
    return;
}
```

Replace the validate loop:
```rust
let validation = match pipeline_ctx.run_validate(&self.middleware).await {
    Ok(v) => v,
    Err(e) => {
        warn!(rig = %self.id, item_id = item.id, error = %e, "validation infra failed, abandoning");
        board.abandon(item.id).await.ok();
        guard.remove().await;
        return;
    }
};
```

- [ ] **Step 4: Verify**

Run: `cargo check --workspace`
Run: `cargo test -p opengoose-rig -- --skip post_execute_npm_check`
Run: `cargo clippy --workspace -- -D warnings`

All must pass cleanly.

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-rig/src/pipeline.rs crates/opengoose-rig/src/rig.rs
git commit -m "refactor(rig): add PipelineContext helpers, remove useless test"
```

---

## Summary of Changes

| Issue | Before | After |
|-------|--------|-------|
| Silent validation infra failure | `.ok()?` → `None` → "passed" | `?` → `Err` → abandon |
| Silent `on_start` failure | `warn!` + continue | `Err` → abandon + return |
| Silent `board.list()` failure | `unwrap_or_default()` | `?` propagation |
| Dead middleware in production | `vec![]` | `vec![ContextHydrator, ValidationGate]` |
| Vague naming | `post_process` | `validate` |
| Useless test | `size_of_val > 0` | Deleted |
| Verbose middleware iteration | Manual loop in rig.rs | `PipelineContext::run_on_start/run_validate` |
