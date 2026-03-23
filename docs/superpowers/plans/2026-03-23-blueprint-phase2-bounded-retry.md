# Blueprint Phase 2-A: BoundedRetry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 검증 실패 시 에러 정보를 LLM에게 전달하고 최대 2회 재시도하여 자동 수정 성공률을 높인다.

**Architecture:** `process_claimed_item()`의 현재 "검증 실패 → abandon" 분기를 retry 루프로 교체. `process()`를 다시 호출하되, 에러 메시지를 포함한 수정 프롬프트를 전달. 최대 2회 재시도 후에도 실패하면 `board.mark_stuck()`으로 전환 (abandon이 아닌 stuck — 사람이 볼 수 있도록).

**Tech Stack:** Rust, tokio, goose Agent API

---

## 설계 결정

### 왜 2회인가?
Stripe Minions 관찰: 1회 재시도로 ~90% 누적 성공, 3회+ 는 수확 체감. LLM은 같은 문제를 반복할수록 같은 실수를 반복하는 경향.

### abandon → mark_stuck 전환
Phase 1에서는 검증 실패 시 `board.abandon()`을 호출했는데, abandon된 작업은 다시 시도되지 않는다. Phase 2에서는 `board.mark_stuck()`으로 전환 — Stuck 상태는 사람이 확인하거나 `board.retry()`로 재시도 가능.

### 세션 재사용
retry 시 같은 세션을 재사용한다. LLM은 이전 시도의 컨텍스트를 유지하고 있으므로, 에러 메시지만 추가하면 된다. 새 세션을 만들면 이전 컨텍스트가 사라져 비효율적.

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `crates/opengoose-rig/src/rig.rs:310-338` | retry 루프 추가 |

단 1개 파일, ~30줄 변경. middleware.rs는 변경 없음 — `post_execute()`는 이미 에러 메시지를 `Option<String>`으로 반환하므로 그대로 재사용.

---

## Task 1: process_claimed_item에 bounded retry 루프 추가

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:310-338`
- Test: `crates/opengoose-rig/src/rig.rs` (inline tests)

현재 코드 (rig.rs:310-338):
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

- [ ] **Step 1: Replace the result handling block with a bounded retry loop**

Replace lines 310-338 in `process_claimed_item()` with:

```rust
const MAX_RETRIES: u32 = 2;

let mut last_result = self.process(input).await;

for attempt in 0..=MAX_RETRIES {
    // LLM 실패 → 즉시 중단 (retry 대상 아님)
    if let Err(ref e) = last_result {
        warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
        board.abandon(item.id).await.ok();
        break;
    }

    // LLM 성공 → 검증
    let validation = crate::middleware::post_execute(&guard.path).await;

    match validation {
        None => {
            // 검증 통과 → submit
            if let Err(e) = board.submit(item.id, &self.id).await {
                warn!(rig = %self.id, item_id = item.id, error = %e, "submit failed");
            } else {
                info!(rig = %self.id, item_id = item.id, "submitted work item");
            }
            break;
        }
        Some(ref validation_error) if attempt < MAX_RETRIES => {
            // 검증 실패 + 재시도 가능 → LLM에게 에러 전달
            warn!(
                rig = %self.id, item_id = item.id,
                attempt = attempt + 1, max = MAX_RETRIES,
                "validation failed, retrying"
            );
            let fix_prompt = format!(
                "The previous implementation failed validation. Please fix the errors:\n\n{}",
                validation_error
            );
            let retry_input = WorkInput::task(fix_prompt, item.id)
                .with_session_id(session_id.clone());
            last_result = self.process(retry_input).await;
        }
        Some(validation_error) => {
            // 검증 실패 + 재시도 소진 → stuck
            warn!(
                rig = %self.id, item_id = item.id,
                error = %validation_error,
                "validation failed after {MAX_RETRIES} retries, marking stuck"
            );
            board.mark_stuck(item.id, &self.id).await.ok();
            break;
        }
    }
}
```

IMPORTANT: `session_id` is consumed by the initial `input` at line 308 via `with_session_id(session_id)`. You must change line 308 to use `session_id.clone()` so `session_id` remains available for the retry loop:
```rust
let input = WorkInput::task(prompt, item.id).with_session_id(session_id.clone());
```

IMPORTANT: Keep `guard.remove().await;` at the end of the function (line 340) — do NOT touch it.

- [ ] **Step 2: Verify session_id is accessible in the retry loop**

The `session_id` variable is declared at line 270 as part of `let (session_id, resuming) = ...`. It's a `String` from `s.id.clone()`. The retry loop uses `session_id.clone()` which requires `session_id: String`. Verify this compiles.

Run: `cargo check -p opengoose-rig`
Expected: PASS

- [ ] **Step 3: Run tests**

Run: `cargo test -p opengoose-rig -- --skip post_execute_npm_check_succeeds`
Expected: ALL PASS. No existing tests exercise `process_claimed_item()` end-to-end, so no breakage expected.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -p opengoose-rig -- -D warnings`
Expected: No warnings

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "feat(rig): add bounded retry loop to Blueprint pipeline (max 2 retries)"
```

---

## Task 2: Verify mark_stuck exists and works

**Files:**
- Read-only: `crates/opengoose-board/src/work_items.rs` (mark_stuck is in `impl Board` block here, not board.rs)

- [ ] **Step 1: Verify Board::mark_stuck() API**

Read `crates/opengoose-board/src/work_items.rs` and confirm:
- `pub async fn mark_stuck(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError>` exists
- It transitions item to `Status::Stuck`
- It verifies `claimed_by` matches `rig_id`

If `mark_stuck` does NOT exist, the plan needs adjustment — use `board.abandon()` instead and note in the commit message.

- [ ] **Step 2: Run full workspace check**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace -- --skip post_execute_npm_check_succeeds`
Expected: ALL PASS

---

## Task 3: Integration verification

- [ ] **Step 1: Run clippy on workspace**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 2: Verify the complete pipeline flow**

Read `process_claimed_item()` and confirm the retry flow:
```
1. WorktreeGuard::create/attach     ← 격리
2. pre_hydrate(board_prime)          ← 컨텍스트 주입
3. Session create/find               ← 세션 관리
4. process(input)                    ← LLM 실행 (1차)
5. post_execute()                    ← 검증
   ├─ 통과 → submit ✅
   ├─ 실패 + attempt < 2 → process(fix_prompt) → goto 5
   └─ 실패 + attempt == 2 → mark_stuck 🛑
6. guard.remove()                    ← 정리
```

- [ ] **Step 3: Final commit if any adjustments**

```bash
git add -u
git commit -m "fix(rig): address clippy warnings in bounded retry"
```
