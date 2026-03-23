# Blueprint Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Worker의 `process_claimed_item()`에 Blueprint 파이프라인 Phase 1을 완성 — Board prime 주입 + cargo test 검증 + 결과 연결.

**Architecture:** 기존 `pre_hydrate()` / `post_execute()` 자유 함수와 `beads::prime_summary()` 를 Worker 파이프라인에 올바르게 연결. 현재 `process_claimed_item()`은 worktree 생성 → LLM 실행 → submit/abandon 만 하는데, 빠진 3개 단계를 추가: (1) Board prime을 시스템 프롬프트에 주입, (2) cargo test를 post_execute에 추가, (3) post_execute 결과를 파이프라인에 반영.

**Tech Stack:** Rust, tokio, goose Agent API, SeaORM/SQLite

---

## 현재 상태 분석

### 이미 구현된 것
- `pre_hydrate()` (`middleware.rs:21`) — AGENTS.md + skill_catalog 주입. **하지만** `process_claimed_item()`에서 호출하지 않음
- `post_execute()` (`middleware.rs:29`) — `cargo check` + `npm test`. **하지만** `cargo test`는 없고, 결과를 파이프라인에 반영하지 않음
- `beads::prime_summary()` (`beads.rs:45`) — Board 요약 텍스트 생성. **하지만** 어디에서도 호출하지 않음
- `WorktreeGuard` (`worktree.rs`) — RAII 격리. 완전 구현
- `Rig::process()` (`rig.rs:58`) — Agent.reply() 호출. 완전 구현

### 빠진 것
1. **prime 주입** — `prime_summary()` 결과를 `agent.extend_system_prompt()`로 주입
2. **pre_hydrate 호출** — `process_claimed_item()`에서 worktree 경로로 `pre_hydrate()` 호출
3. **cargo test** — `post_execute()`에 `cargo test` 단계 추가
4. **post_execute 연결** — LLM 실행 후 `post_execute()` 호출하고 실패 시 abandon

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `crates/opengoose-rig/src/middleware.rs` | cargo test 추가, prime 주입 함수 추가 |
| Modify | `crates/opengoose-rig/src/rig.rs` | process_claimed_item()에 pre_hydrate + prime + post_execute 연결 |
| Read-only | `crates/opengoose-board/src/beads.rs` | prime_summary() 이미 구현됨, 호출만 하면 됨 |

---

## Task 1: middleware에 prime 주입 함수 추가

**Files:**
- Modify: `crates/opengoose-rig/src/middleware.rs`
- Test: `crates/opengoose-rig/src/middleware.rs` (inline tests)

- [ ] **Step 1: Write the failing test for prime injection**

```rust
#[tokio::test]
async fn hydration_context_includes_board_prime() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = hydration_context(tmp.path(), "", "Board: 3 open, 1 claimed, 2 done\nRig: worker\n");
    assert_eq!(ctx.len(), 1);
    assert_eq!(ctx[0].0, "board-prime");
    assert!(ctx[0].1.contains("3 open"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opengoose-rig hydration_context_includes_board_prime -- --nocapture`
Expected: FAIL — `hydration_context` doesn't accept 3rd parameter

- [ ] **Step 3: Add board_prime parameter to hydration_context**

Modify `hydration_context()` signature and body in `middleware.rs`:

```rust
fn hydration_context(work_dir: &Path, skill_catalog: &str, board_prime: &str) -> Vec<(String, String)> {
    let mut ctx = Vec::new();
    if let Some(agents_md) = load_agents_md(work_dir) {
        ctx.push(("agents-md".to_string(), agents_md));
    }
    if !skill_catalog.is_empty() {
        ctx.push(("skill-catalog".to_string(), skill_catalog.to_string()));
    }
    if !board_prime.is_empty() {
        ctx.push(("board-prime".to_string(), board_prime.to_string()));
    }
    ctx
}
```

Update `pre_hydrate()` to accept and pass `board_prime`:

```rust
pub async fn pre_hydrate(agent: &Agent, work_dir: &Path, skill_catalog: &str, board_prime: &str) {
    for (key, value) in hydration_context(work_dir, skill_catalog, board_prime) {
        agent.extend_system_prompt(key, value).await;
    }
}
```

Update all existing tests that call `hydration_context()` or `pre_hydrate()` to pass `""` as the new 4th/3rd parameter.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p opengoose-rig -- --nocapture`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-rig/src/middleware.rs
git commit -m "feat(rig): add board_prime parameter to pre_hydrate"
```

---

## Task 2: post_execute에 cargo test 추가

**Files:**
- Modify: `crates/opengoose-rig/src/middleware.rs`
- Test: `crates/opengoose-rig/src/middleware.rs` (inline tests)

- [ ] **Step 1: Write the failing test for cargo test execution**

```rust
#[tokio::test]
async fn post_execute_runs_cargo_test_after_check() {
    let tmp = tempfile::tempdir().unwrap();
    // Create a valid Cargo project with a failing test
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"test-proj\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn it_fails() { assert!(false); }
        }
    "#).unwrap();
    let result = post_execute(tmp.path()).await;
    assert!(result.is_some());
    assert!(result.unwrap().contains("cargo test failed"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opengoose-rig post_execute_runs_cargo_test_after_check -- --nocapture`
Expected: FAIL — current `post_execute()` only runs `cargo check`, which passes for this code. The failing test wouldn't be caught.

- [ ] **Step 3: Add cargo test to post_execute**

Modify `run_check()` in `middleware.rs` to also run `cargo test` after `cargo check` passes:

```rust
async fn run_check(work_dir: &Path) -> Option<String> {
    // Step 1: cargo check
    let check_output = tokio::process::Command::new("cargo")
        .arg("check")
        .arg("--message-format=short")
        .current_dir(work_dir)
        .output()
        .await
        .ok()?;

    if !check_output.status.success() {
        let stderr = String::from_utf8_lossy(&check_output.stderr);
        return Some(format!("cargo check failed:\n{stderr}"));
    }

    // Step 2: cargo test
    let test_output = tokio::process::Command::new("cargo")
        .arg("test")
        .current_dir(work_dir)
        .output()
        .await
        .ok()?;

    if !test_output.status.success() {
        let stderr = String::from_utf8_lossy(&test_output.stderr);
        let stdout = String::from_utf8_lossy(&test_output.stdout);
        return Some(format!("cargo test failed:\n{stdout}\n{stderr}"));
    }

    None
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p opengoose-rig -- --nocapture`
Expected: ALL PASS. The existing `post_execute_returns_none_when_cargo_check_passes` test should still pass (empty lib.rs has no tests to fail).

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-rig/src/middleware.rs
git commit -m "feat(rig): add cargo test to post_execute validation"
```

---

## Task 3: Worker 파이프라인에 pre_hydrate + prime + post_execute 연결

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:239-322` (`process_claimed_item`)
- Test: `crates/opengoose-rig/src/rig.rs` (inline tests)

- [ ] **Step 1: Add Board import for prime_summary**

Add to rig.rs imports:

```rust
use opengoose_board::beads;
```

- [ ] **Step 2: Add pre_hydrate + prime to process_claimed_item**

In `process_claimed_item()`, after worktree guard creation and before session creation, add:

```rust
// Blueprint Phase 1: pre_hydrate — AGENTS.md + Skills + Board 요약 주입
let all_items = board.list().await.unwrap_or_default();
let board_prime = beads::prime_summary(&all_items, &self.id);
crate::middleware::pre_hydrate(&self.agent, &guard.path, "", &board_prime).await;
```

Insert this block after line 261 (after worktree guard is created) and before line 263 (session lookup).

- [ ] **Step 3: Add post_execute after process() call**

In `process_claimed_item()`, replace the simple `result` match block (lines 305-319) with:

```rust
let result = self.process(input).await;

// Blueprint Phase 1: post_execute — cargo check + test
let validation = if result.is_ok() {
    crate::middleware::post_execute(&guard.path).await
} else {
    None
};

match (&result, &validation) {
    (Ok(()), None) => {
        // LLM 성공 + 검증 통과 → submit
        if let Err(e) = board.submit(item.id, &self.id).await {
            warn!(rig = %self.id, item_id = item.id, error = %e, "submit failed");
        } else {
            info!(rig = %self.id, item_id = item.id, "submitted work item");
        }
    }
    (Ok(()), Some(validation_error)) => {
        // LLM 성공 + 검증 실패 → abandon (Phase 2에서 retry로 전환)
        warn!(rig = %self.id, item_id = item.id, error = %validation_error, "validation failed, abandoning");
        board.abandon(item.id).await.ok();
    }
    (Err(e), _) => {
        // LLM 실패 → abandon
        warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
        board.abandon(item.id).await.ok();
    }
}
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test -p opengoose-rig -- --nocapture`
Expected: ALL PASS. Existing tests don't exercise `process_claimed_item()` end-to-end (they test components individually), so no breakage expected.

- [ ] **Step 5: Run cargo check on workspace**

Run: `cargo check --workspace`
Expected: PASS — all imports resolve, all signatures match.

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "feat(rig): wire Blueprint Phase 1 pipeline in process_claimed_item"
```

---

## Task 4: 통합 검증

- [ ] **Step 1: Run full workspace tests**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Verify the complete pipeline flow**

Read `process_claimed_item()` and confirm the order:
```
1. WorktreeGuard::create/attach     ← 격리
2. pre_hydrate(board_prime)          ← 컨텍스트 주입 (NEW)
3. Session create/find               ← 세션 관리
4. process(input)                    ← LLM 실행
5. post_execute(cargo check + test)  ← 검증 (ENHANCED)
6. submit or abandon                 ← 결과 제출 (ENHANCED)
7. guard.remove()                    ← 정리
```

- [ ] **Step 4: Final commit if any adjustments needed**

```bash
git add -u
git commit -m "fix(rig): address clippy warnings in blueprint pipeline"
```
