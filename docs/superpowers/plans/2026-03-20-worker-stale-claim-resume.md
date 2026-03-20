# Worker Stale Claim Resume Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Worker가 재시작 시 이전에 claim한 아이템을 세션 연속성과 함께 resume하도록 한다.

**Architecture:** Board에 `claimed_by()` 쿼리 메서드 추가, Worker의 `try_claim_and_execute()` 내부를 `process_claimed_item()`으로 추출, `run()` 시작부에 resume 루프 삽입.

**Tech Stack:** Rust, SeaORM, tokio, goose SessionManager

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/opengoose-board/src/board.rs` | Modify | `claimed_by()` 메서드 추가 |
| `crates/opengoose-rig/src/rig.rs` | Modify | `process_claimed_item()` 추출, `run()` resume 루프 |

---

### Task 1: Board — `claimed_by()` 메서드 + 테스트

**Files:**
- Modify: `crates/opengoose-board/src/board.rs:236-251` (ready() 바로 아래)

- [ ] **Step 1: Write failing test**

`crates/opengoose-board/src/board.rs` 하단 `mod tests` 블록에 추가:

```rust
#[tokio::test]
async fn claimed_by_returns_items_claimed_by_rig() {
    let board = new_board().await;
    let rig_a = RigId::new("worker-a");
    let rig_b = RigId::new("worker-b");

    board.post(post_req("task-1")).await.unwrap();
    board.post(post_req("task-2")).await.unwrap();
    board.post(post_req("task-3")).await.unwrap();

    board.claim(1, &rig_a).await.unwrap();
    board.claim(2, &rig_b).await.unwrap();
    board.claim(3, &rig_a).await.unwrap();

    let items = board.claimed_by(&rig_a).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, 1);
    assert_eq!(items[1].id, 3);

    let items_b = board.claimed_by(&rig_b).await.unwrap();
    assert_eq!(items_b.len(), 1);
    assert_eq!(items_b[0].id, 2);

    // 아무도 claim하지 않은 rig → 빈 벡터
    let empty = board.claimed_by(&RigId::new("nobody")).await.unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn claimed_by_sorts_by_priority_desc() {
    let board = new_board().await;
    let rig = RigId::new("worker");

    // P2(낮음) 먼저 생성
    board.post(PostWorkItem {
        title: "low".to_string(),
        description: String::new(),
        created_by: RigId::new("user"),
        priority: Priority::P2,
        tags: vec![],
    }).await.unwrap();
    // P0(높음) 나중에 생성
    board.post(PostWorkItem {
        title: "high".to_string(),
        description: String::new(),
        created_by: RigId::new("user"),
        priority: Priority::P0,
        tags: vec![],
    }).await.unwrap();

    board.claim(1, &rig).await.unwrap();
    board.claim(2, &rig).await.unwrap();

    let items = board.claimed_by(&rig).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "high");  // P0 먼저
    assert_eq!(items[1].title, "low");   // P2 나중
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opengoose-board claimed_by`
Expected: FAIL — `claimed_by` method not found

- [ ] **Step 3: Implement `claimed_by()`**

`crates/opengoose-board/src/board.rs`의 `ready()` 메서드 아래에 추가:

```rust
/// 특정 rig이 claim한 아이템 조회. priority 내림차순.
pub async fn claimed_by(&self, rig_id: &RigId) -> Result<Vec<WorkItem>, BoardError> {
    let mut items: Vec<WorkItem> = entity::work_item::Entity::find()
        .filter(entity::work_item::Column::Status.eq(Status::Claimed.to_value()))
        .filter(entity::work_item::Column::ClaimedBy.eq(&rig_id.0))
        .all(&self.db)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(WorkItem::from)
        .collect();

    items.sort_by(|a, b| b.priority.urgency().cmp(&a.priority.urgency()));
    Ok(items)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p opengoose-board claimed_by`
Expected: PASS — 두 테스트 모두 통과

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-board/src/board.rs
git commit -m "feat(board): add claimed_by() query method"
```

---

### Task 2: Worker — `process_claimed_item()` 추출

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:191-231`

- [ ] **Step 1: Extract `process_claimed_item()` from `try_claim_and_execute()`**

`crates/opengoose-rig/src/rig.rs`의 `impl Worker` 블록에 새 메서드 추가. `try_claim_and_execute()`의 라인 202-228(세션 생성 → process → submit/abandon)을 추출:

```rust
/// claim된 아이템을 처리. 세션 조회/생성 → process → submit or abandon.
/// 에러는 내부에서 처리하고 호출자에게 전파하지 않음.
async fn process_claimed_item(&self, item: &WorkItem, board: &Arc<Board>) {
    let session_name = format!("task-{}", item.id);

    // 기존 세션 조회 → 없으면 새로 생성
    let session_id = match self.find_session_by_name(&session_name).await {
        Some(id) => {
            info!(rig = %self.id, item_id = item.id, "resuming existing session");
            id
        }
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
            match self
                .agent
                .config
                .session_manager
                .create_session(
                    cwd,
                    session_name,
                    goose::session::session_manager::SessionType::User,
                )
                .await
            {
                Ok(s) => s.id,
                Err(e) => {
                    warn!(rig = %self.id, item_id = item.id, error = %e, "failed to create session, abandoning");
                    board.abandon(item.id).await.ok();
                    return;
                }
            }
        }
    };

    let input = WorkInput::task(
        format!("Work item #{}: {}\n\n{}", item.id, item.title, item.description),
        item.id,
    )
    .with_session_id(session_id);

    let result = self.process(input).await;
    match result {
        Ok(()) => {
            board.submit(item.id, &self.id).await.ok();
            info!(rig = %self.id, item_id = item.id, "submitted work item");
        }
        Err(e) => {
            warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
            board.abandon(item.id).await.ok();
        }
    }
}

/// goose session_manager에서 name으로 세션 조회. 마지막(최신) 매칭 반환.
async fn find_session_by_name(&self, name: &str) -> Option<String> {
    let sessions = self
        .agent
        .config
        .session_manager
        .list_sessions()
        .await
        .ok()?;
    sessions
        .iter()
        .rev()
        .find(|s| s.name == name)
        .map(|s| s.id.clone())
}
```

- [ ] **Step 2: Rewrite `try_claim_and_execute()` to use `process_claimed_item()`**

```rust
async fn try_claim_and_execute(&self) -> anyhow::Result<bool> {
    let board = self.board.as_ref().expect("Worker must have a board");
    let ready = board.ready().await?;

    let Some(item) = ready.first() else {
        return Ok(false);
    };

    let item = board.claim(item.id, &self.id).await?;
    info!(rig = %self.id, item_id = item.id, title = %item.title, "claimed work item");

    self.process_claimed_item(&item, board).await;
    Ok(true)
}
```

- [ ] **Step 3: Verify existing tests pass**

Run: `cargo test -p opengoose-rig`
Expected: PASS — 기존 `extract_text_content_keeps_newline_between_segments` 테스트 통과

Run: `cargo check -p opengoose`
Expected: 컴파일 성공

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "refactor(rig): extract process_claimed_item from try_claim_and_execute"
```

---

### Task 3: Worker — resume 루프 추가

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:160-188`

- [ ] **Step 1: Add resume phase to `run()`**

`run()` 메서드의 `info!("worker started")` 이후, `loop` 진입 전에 resume 단계 삽입:

```rust
pub async fn run(&self) {
    let Some(board) = &self.board else {
        warn!(rig = %self.id, "worker has no board, exiting");
        return;
    };
    info!(rig = %self.id, "worker started, waiting for work");

    // Phase 1: Resume — 이전에 claim한 아이템 처리
    let stale = board.claimed_by(&self.id).await.unwrap_or_default();
    if !stale.is_empty() {
        info!(rig = %self.id, count = stale.len(), "resuming previously claimed items");
    }
    for item in &stale {
        if self.cancel.is_cancelled() { break; }
        self.process_claimed_item(item, board).await;
    }

    // Phase 2: Pull loop (기존 코드, 변경 없음)
    loop {
        // ... (unchanged)
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p opengoose`
Expected: 컴파일 성공

- [ ] **Step 3: Full test suite**

Run: `cargo test`
Expected: 모든 테스트 통과

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "feat(rig): resume previously claimed items on worker restart"
```
