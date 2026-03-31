# WorkItem Sub-Task Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** WorkItem에 `parent_id` 필드를 추가하여 1단계 깊이의 sub-task 트리를 지원하고, 모든 자식이 Done이면 부모가 자동 완료되도록 한다.

**Architecture:** `parent_id: Option<i64>` 컬럼을 work_items 테이블에 추가. `post()`에서 깊이 검증, `submit()`에서 자동 완료 트리거, `children()` 쿼리 추가. CLI에 `--parent` 옵션과 `children` 서브커맨드. MCP `create_task`에 `parent_id` 파라미터.

**Tech Stack:** Rust, SeaORM, SQLite, clap

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `crates/opengoose-board/src/entity/work_item.rs` | SeaORM entity — DB 컬럼 정의 | Modify: `parent_id` 컬럼 추가 |
| `crates/opengoose-board/src/work_item.rs` | WorkItem struct + PostWorkItem + BoardError | Modify: `parent_id` 필드, 새 에러 variants |
| `crates/opengoose-board/src/board.rs` | Board — 테이블 생성, 마이그레이션 | Modify: `ensure_columns`에 ALTER TABLE 추가 |
| `crates/opengoose-board/src/work_items/transitions.rs` | 상태 전이 (post, submit, ...) | Modify: `post()` 깊이 검증, `submit()` 자동 완료 |
| `crates/opengoose-board/src/work_items/queries.rs` | 읽기 전용 쿼리 | Modify: `children()` 추가 |
| `crates/opengoose-board/src/test_helpers.rs` | 테스트 헬퍼 | Modify: `post_req_with_parent()` 추가 |
| `crates/opengoose/src/cli/mod.rs` | CLI 정의 | Modify: `--parent`, `Children` 서브커맨드 |
| `crates/opengoose/src/commands/board.rs` | CLI 핸들러 | Modify: create/children 핸들링 |
| `crates/opengoose-rig/src/mcp_tools/schema.rs` | MCP tool 스키마 | Modify: `create_task`에 `parent_id` |
| `crates/opengoose-rig/src/mcp_tools/handlers.rs` | MCP tool 핸들러 | Modify: `handle_create_task`에 `parent_id` |

---

### Task 1: DB Entity + WorkItem struct에 parent_id 추가

**Files:**
- Modify: `crates/opengoose-board/src/entity/work_item.rs`
- Modify: `crates/opengoose-board/src/work_item.rs`

- [ ] **Step 1: entity/work_item.rs에 parent_id 컬럼 추가**

`crates/opengoose-board/src/entity/work_item.rs`의 `Model` struct에 추가:

```rust
pub parent_id: Option<i64>,
```

`updated_at` 필드 뒤에 추가.

`From<Model> for WorkItem` impl에서 매핑 추가:

```rust
impl From<Model> for WorkItem {
    fn from(m: Model) -> Self {
        let tags: Vec<String> = m
            .tags
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        WorkItem {
            id: m.id,
            title: m.title,
            description: m.description,
            status: m.status,
            priority: m.priority,
            tags,
            created_by: RigId::new(m.created_by),
            claimed_by: m.claimed_by.map(RigId::new),
            created_at: m.created_at,
            updated_at: m.updated_at,
            parent_id: m.parent_id,
        }
    }
}
```

- [ ] **Step 2: WorkItem struct에 parent_id 필드 추가**

`crates/opengoose-board/src/work_item.rs`의 `WorkItem` struct에 추가 (updated_at 뒤):

```rust
pub parent_id: Option<i64>,
```

주석 `Phase 후반에 추가: project, parent, session_id, ...`에서 `parent` 제거.

- [ ] **Step 3: PostWorkItem에 parent_id 필드 추가**

`crates/opengoose-board/src/work_item.rs`의 `PostWorkItem` struct에 추가:

```rust
pub parent_id: Option<i64>,
```

- [ ] **Step 4: BoardError에 새 variants 추가**

`crates/opengoose-board/src/work_item.rs`의 `BoardError` enum에 추가:

```rust
#[error("parent not found: {0}")]
ParentNotFound(i64),

#[error("max sub-task depth exceeded: parent {parent_id} is already a sub-task")]
MaxDepthExceeded { parent_id: i64 },

#[error("parent {parent_id} is already completed")]
ParentCompleted { parent_id: i64 },
```

- [ ] **Step 5: 컴파일 에러 확인 및 수정**

Run: `cargo check -p opengoose-board 2>&1 | head -40`

Expected: 여러 컴파일 에러 — `WorkItem` 리터럴에 `parent_id` 필드 누락, `PostWorkItem` 리터럴에 누락. 이것들은 다음 Task에서 수정.

- [ ] **Step 6: Commit (컴파일 안 될 수 있음 — WIP)**

```bash
git add crates/opengoose-board/src/entity/work_item.rs crates/opengoose-board/src/work_item.rs
git commit -m "wip: add parent_id field to WorkItem and PostWorkItem structs"
```

---

### Task 2: 기존 코드에서 parent_id 필드 채우기 + 마이그레이션

**Files:**
- Modify: `crates/opengoose-board/src/board.rs`
- Modify: `crates/opengoose-board/src/work_items/transitions.rs`
- Modify: `crates/opengoose-board/src/test_helpers.rs`
- Modify: 기타 `WorkItem` 리터럴이 있는 모든 테스트 파일

- [ ] **Step 1: ensure_columns에 ALTER TABLE 추가**

`crates/opengoose-board/src/board.rs`의 `ensure_columns`에 추가:

```rust
let stmts = [
    "ALTER TABLE stamps ADD COLUMN active_skill_versions TEXT",
    "ALTER TABLE work_items ADD COLUMN parent_id INTEGER",
];
```

- [ ] **Step 2: post()에서 parent_id 저장**

`crates/opengoose-board/src/work_items/transitions.rs`의 `post()` 메서드에서 ActiveModel 생성 부분에 추가:

```rust
let model = entity::work_item::ActiveModel {
    id: NotSet,
    title: Set(req.title),
    description: Set(req.description),
    status: Set(Status::Open),
    priority: Set(req.priority),
    tags: Set(tags_json),
    created_by: Set(req.created_by.0),
    claimed_by: Set(None),
    created_at: Set(now),
    updated_at: Set(now),
    parent_id: Set(req.parent_id),
};
```

- [ ] **Step 3: test_helpers.rs의 post_req에 parent_id 추가**

`crates/opengoose-board/src/test_helpers.rs`의 `post_req`:

```rust
pub fn post_req(title: &str) -> PostWorkItem {
    PostWorkItem {
        title: title.to_string(),
        description: String::new(),
        created_by: RigId::new("user"),
        priority: Priority::P1,
        tags: vec![],
        parent_id: None,
    }
}
```

새 헬퍼 추가:

```rust
pub fn post_req_with_parent(title: &str, parent_id: i64) -> PostWorkItem {
    PostWorkItem {
        title: title.to_string(),
        description: String::new(),
        created_by: RigId::new("user"),
        priority: Priority::P1,
        tags: vec![],
        parent_id: Some(parent_id),
    }
}
```

- [ ] **Step 4: 모든 WorkItem 리터럴과 PostWorkItem 리터럴에 parent_id 추가**

프로젝트 전체에서 `PostWorkItem {` 리터럴을 검색하여 `parent_id: None,`을 추가. `WorkItem {` 리터럴도 동일하게 `parent_id: None,` 추가.

Run: `grep -rn "PostWorkItem {" --include="*.rs" | grep -v test_helpers`

각 리터럴에 `parent_id: None,` 추가.

Run: `grep -rn "WorkItem {" --include="*.rs" crates/ | grep -v "use\|entity\|Entity\|PostWork"`

각 리터럴에 `parent_id: None,` 추가.

- [ ] **Step 5: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공 (경고만 가능)

- [ ] **Step 6: cargo test 통과 확인**

Run: `cargo test`
Expected: 전부 PASS

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(board): add parent_id column + migration + fix all literals"
```

---

### Task 3: post() 깊이 검증

**Files:**
- Modify: `crates/opengoose-board/src/work_items/transitions.rs`

- [ ] **Step 1: 깊이 검증 테스트 작성**

`crates/opengoose-board/src/work_items/transitions.rs` 하단 tests 모듈에 추가:

```rust
#[tokio::test]
async fn post_with_parent_creates_subtask() {
    let board = new_board().await;
    let parent = board.post(post_req("parent")).await.expect("post parent");
    let child = board
        .post(PostWorkItem {
            title: "child".into(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
            parent_id: Some(parent.id),
        })
        .await
        .expect("post child");
    assert_eq!(child.parent_id, Some(parent.id));
}

#[tokio::test]
async fn post_rejects_depth_2_subtask() {
    let board = new_board().await;
    let parent = board.post(post_req("parent")).await.expect("post parent");
    let child = board
        .post(PostWorkItem {
            title: "child".into(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
            parent_id: Some(parent.id),
        })
        .await
        .expect("post child");

    let result = board
        .post(PostWorkItem {
            title: "grandchild".into(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
            parent_id: Some(child.id),
        })
        .await;
    assert!(
        matches!(result, Err(BoardError::MaxDepthExceeded { .. })),
        "expected MaxDepthExceeded, got {result:?}"
    );
}

#[tokio::test]
async fn post_rejects_nonexistent_parent() {
    let board = new_board().await;
    let result = board
        .post(PostWorkItem {
            title: "orphan".into(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
            parent_id: Some(999),
        })
        .await;
    assert!(
        matches!(result, Err(BoardError::ParentNotFound(999))),
        "expected ParentNotFound, got {result:?}"
    );
}

#[tokio::test]
async fn post_rejects_done_parent() {
    let board = new_board().await;
    let parent = board.post(post_req("parent")).await.expect("post parent");
    board.claim(parent.id, &RigId::new("w")).await.expect("claim");
    board.submit(parent.id, &RigId::new("w")).await.expect("submit");

    let result = board
        .post(PostWorkItem {
            title: "child of done".into(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
            parent_id: Some(parent.id),
        })
        .await;
    assert!(
        matches!(result, Err(BoardError::ParentCompleted { .. })),
        "expected ParentCompleted, got {result:?}"
    );
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p opengoose-board post_with_parent post_rejects`
Expected: FAIL — 검증 로직 없음

- [ ] **Step 3: post()에 깊이 검증 구현**

`crates/opengoose-board/src/work_items/transitions.rs`의 `post()` 메서드 시작 부분 (tags_json 생성 전)에 추가:

```rust
// Validate parent_id
if let Some(pid) = req.parent_id {
    let parent = self.get(pid).await?.ok_or(BoardError::ParentNotFound(pid))?;
    if parent.parent_id.is_some() {
        return Err(BoardError::MaxDepthExceeded { parent_id: pid });
    }
    if matches!(parent.status, Status::Done | Status::Abandoned) {
        return Err(BoardError::ParentCompleted { parent_id: pid });
    }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p opengoose-board post_with_parent post_rejects`
Expected: 4 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-board/src/work_items/transitions.rs
git commit -m "feat(board): validate parent_id depth and status on post()"
```

---

### Task 4: children() 쿼리

**Files:**
- Modify: `crates/opengoose-board/src/work_items/queries.rs`

- [ ] **Step 1: children 테스트 작성**

`crates/opengoose-board/src/work_items/queries.rs` 하단 tests 모듈에 추가:

```rust
#[tokio::test]
async fn children_returns_subtasks() {
    let board = new_board().await;
    let parent = board.post(post_req("parent")).await.expect("post parent");
    board
        .post(crate::test_helpers::post_req_with_parent("child-1", parent.id))
        .await
        .expect("post child-1");
    board
        .post(crate::test_helpers::post_req_with_parent("child-2", parent.id))
        .await
        .expect("post child-2");
    board
        .post(post_req("unrelated"))
        .await
        .expect("post unrelated");

    let kids = board.children(parent.id).await.expect("children query");
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].title, "child-1");
    assert_eq!(kids[1].title, "child-2");
}

#[tokio::test]
async fn children_returns_empty_for_no_subtasks() {
    let board = new_board().await;
    let item = board.post(post_req("no kids")).await.expect("post");
    let kids = board.children(item.id).await.expect("children query");
    assert!(kids.is_empty());
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p opengoose-board children_returns`
Expected: FAIL — `children` 메서드 없음

- [ ] **Step 3: children() 구현**

`crates/opengoose-board/src/work_items/queries.rs`의 `impl Board`에 추가:

```rust
/// 특정 작업의 sub-task 조회 (id ASC).
pub async fn children(&self, parent_id: i64) -> Result<Vec<WorkItem>, BoardError> {
    entity::work_item::Entity::find()
        .filter(entity::work_item::Column::ParentId.eq(parent_id))
        .order_by_asc(entity::work_item::Column::Id)
        .all(&self.db)
        .await
        .map(|models| models.into_iter().map(WorkItem::from).collect())
        .map_err(db_err)
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p opengoose-board children_returns`
Expected: 2 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-board/src/work_items/queries.rs
git commit -m "feat(board): add children() query for sub-tasks"
```

---

### Task 5: submit() 자동 완료

**Files:**
- Modify: `crates/opengoose-board/src/work_items/transitions.rs`

- [ ] **Step 1: 자동 완료 테스트 작성**

`crates/opengoose-board/src/work_items/transitions.rs` 하단 tests 모듈에 추가:

```rust
#[tokio::test]
async fn submit_last_child_auto_completes_parent() {
    let board = new_board().await;
    let parent = board.post(post_req("parent")).await.expect("post parent");
    board.claim(parent.id, &RigId::new("w")).await.expect("claim parent");

    let c1 = board
        .post(crate::test_helpers::post_req_with_parent("child-1", parent.id))
        .await
        .expect("post child-1");
    let c2 = board
        .post(crate::test_helpers::post_req_with_parent("child-2", parent.id))
        .await
        .expect("post child-2");

    // Submit child-1 — parent should NOT auto-complete yet
    board.claim(c1.id, &RigId::new("w1")).await.expect("claim c1");
    board.submit(c1.id, &RigId::new("w1")).await.expect("submit c1");
    let parent_after_c1 = board.get(parent.id).await.expect("get").expect("parent exists");
    assert_eq!(parent_after_c1.status, Status::Claimed, "parent should still be Claimed");

    // Submit child-2 — parent should auto-complete
    board.claim(c2.id, &RigId::new("w2")).await.expect("claim c2");
    board.submit(c2.id, &RigId::new("w2")).await.expect("submit c2");
    let parent_after_c2 = board.get(parent.id).await.expect("get").expect("parent exists");
    assert_eq!(parent_after_c2.status, Status::Done, "parent should auto-complete");
}

#[tokio::test]
async fn abandoned_child_prevents_parent_auto_complete() {
    let board = new_board().await;
    let parent = board.post(post_req("parent")).await.expect("post parent");
    board.claim(parent.id, &RigId::new("w")).await.expect("claim parent");

    let c1 = board
        .post(crate::test_helpers::post_req_with_parent("child-1", parent.id))
        .await
        .expect("post child-1");
    let c2 = board
        .post(crate::test_helpers::post_req_with_parent("child-2", parent.id))
        .await
        .expect("post child-2");

    // Abandon child-1
    board.abandon(c1.id).await.expect("abandon c1");

    // Submit child-2 — parent should NOT auto-complete (c1 is Abandoned)
    board.claim(c2.id, &RigId::new("w")).await.expect("claim c2");
    board.submit(c2.id, &RigId::new("w")).await.expect("submit c2");
    let parent_state = board.get(parent.id).await.expect("get").expect("parent exists");
    assert_eq!(parent_state.status, Status::Claimed, "parent should NOT auto-complete");
}

#[tokio::test]
async fn submit_top_level_item_does_not_trigger_auto_complete() {
    let board = new_board().await;
    let item = board.post(post_req("standalone")).await.expect("post");
    board.claim(item.id, &RigId::new("w")).await.expect("claim");
    let done = board.submit(item.id, &RigId::new("w")).await.expect("submit");
    assert_eq!(done.status, Status::Done);
    // No crash, no side effects — just normal submit
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p opengoose-board submit_last_child abandoned_child submit_top_level_item_does_not`
Expected: FAIL — 자동 완료 로직 없음 (submit_last_child_auto_completes_parent 실패)

- [ ] **Step 3: submit()에 자동 완료 로직 추가**

`crates/opengoose-board/src/work_items/transitions.rs`의 `submit()` 메서드를 수정:

```rust
pub async fn submit(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
    let result = self
        .transition(
            item_id,
            Status::Done,
            |item| item.verify_claimed_by(rig_id),
            |_| {},
        )
        .await?;

    // Auto-complete parent if all siblings are Done
    if let Some(pid) = result.parent_id {
        let siblings = self.children(pid).await?;
        let all_done = siblings.iter().all(|s| s.status == Status::Done);
        if all_done {
            let parent = self.get_or_err(pid).await?;
            if parent.status.can_transition_to(Status::Done) {
                let _ = self
                    .transition(pid, Status::Done, |_| Ok(()), |_| {})
                    .await;
            }
        }
    }

    self.notify.notify_waiters();
    Ok(result)
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p opengoose-board submit_last_child abandoned_child submit_top_level_item_does_not`
Expected: 3 tests PASS

- [ ] **Step 5: 전체 테스트 확인**

Run: `cargo test`
Expected: 전부 PASS

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-board/src/work_items/transitions.rs
git commit -m "feat(board): auto-complete parent when all children are Done"
```

---

### Task 6: CLI — --parent 옵션 + children 서브커맨드

**Files:**
- Modify: `crates/opengoose/src/cli/mod.rs`
- Modify: `crates/opengoose/src/commands/board.rs`

- [ ] **Step 1: BoardAction에 --parent와 Children 추가**

`crates/opengoose/src/cli/mod.rs`의 `BoardAction::Create`에 `parent` 필드 추가:

```rust
Create {
    title: String,
    #[arg(long, default_value = "P1")]
    priority: String,
    #[arg(long, value_delimiter = ',')]
    tags: Vec<String>,
    /// Parent work item ID (creates a sub-task)
    #[arg(long)]
    parent: Option<i64>,
},
```

`BoardAction`에 새 variant 추가:

```rust
/// sub-task 목록 조회
Children { id: i64 },
```

- [ ] **Step 2: commands/board.rs에서 Create 핸들링에 parent_id 전달**

`crates/opengoose/src/commands/board.rs`의 `run_board_command`에서 `BoardAction::Create` 매칭을 수정하여 `parent` 파라미터를 `PostWorkItem.parent_id`에 전달. (현재 코드를 읽어서 정확한 위치 확인 필요)

`BoardAction::Children { id }`에 대한 핸들러 추가:

```rust
BoardAction::Children { id } => {
    let children = board.children(id).await?;
    if children.is_empty() {
        println!("No sub-tasks for #{id}");
    } else {
        println!("Sub-tasks for #{id}:");
        for child in &children {
            println!("  #{} {:?} [{}] \"{}\"", child.id, child.priority, child.status, child.title);
        }
    }
    Ok(())
}
```

- [ ] **Step 3: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 4: 기존 CLI 테스트 통과 확인**

Run: `cargo test -p opengoose`
Expected: 전부 PASS (기존 `parent: None` 추가 필요할 수 있음)

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/cli/mod.rs crates/opengoose/src/commands/board.rs
git commit -m "feat(cli): --parent option + board children subcommand"
```

---

### Task 7: MCP create_task에 parent_id 지원

**Files:**
- Modify: `crates/opengoose-rig/src/mcp_tools/schema.rs`
- Modify: `crates/opengoose-rig/src/mcp_tools/handlers.rs`

- [ ] **Step 1: 테스트 작성**

`crates/opengoose-rig/src/mcp_tools/handlers.rs` 하단 tests 모듈에 추가:

```rust
#[tokio::test]
async fn create_task_with_parent_id() {
    let board = Arc::new(
        Board::in_memory()
            .await
            .expect("in-memory board should initialize"),
    );
    let rig_id = RigId::new("test-rig");

    // Create parent
    let mut args = JsonObject::new();
    args.insert("title".into(), json!("parent task"));
    handle_create_task(&board, &rig_id, &args).await;

    // Create child with parent_id
    let mut args = JsonObject::new();
    args.insert("title".into(), json!("child task"));
    args.insert("parent_id".into(), json!(1));
    let result = handle_create_task(&board, &rig_id, &args).await;
    let text = content_text(&result);
    assert!(text.contains("Created #2"));

    let child = board.get(2).await.expect("get").expect("child exists");
    assert_eq!(child.parent_id, Some(1));
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p opengoose-rig create_task_with_parent`
Expected: FAIL

- [ ] **Step 3: schema.rs에 parent_id 파라미터 추가**

`crates/opengoose-rig/src/mcp_tools/schema.rs`의 `create_task` tool 정의에서 properties에 추가:

```rust
"parent_id": {"type": "integer", "description": "Parent work item ID for sub-tasks"}
```

- [ ] **Step 4: handlers.rs에서 parent_id 파싱 + 전달**

`crates/opengoose-rig/src/mcp_tools/handlers.rs`의 `handle_create_task`에서 parent_id 파싱 추가:

```rust
let parent_id = args.get("parent_id").and_then(Value::as_i64);
```

그리고 `PostWorkItem` 생성 시 전달:

```rust
board
    .post(PostWorkItem {
        title: title.to_string(),
        description,
        created_by: rig_id.clone(),
        priority,
        tags: vec![],
        parent_id,
    })
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test -p opengoose-rig create_task_with_parent`
Expected: PASS

- [ ] **Step 6: 전체 테스트 확인**

Run: `cargo test`
Expected: 전부 PASS

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose-rig/src/mcp_tools/schema.rs crates/opengoose-rig/src/mcp_tools/handlers.rs
git commit -m "feat(mcp): add parent_id to create_task tool"
```

---

### Task 8: ARCHITECTURE.md 업데이트

**Files:**
- Modify: `docs/v0.2/ARCHITECTURE.md`

- [ ] **Step 1: §14 열린 질문 3번 업데이트**

`WorkItem 확장 필드` 항목에서 `parent` 제거하고 해결됨 표시:

기존:
```
3. **WorkItem 확장 필드?** `project`, `parent`, `session_id`, `seq`, `assigned_to`, `notes`, `result` — Phase 후반.
```

변경:
```
3. **WorkItem 확장 필드?** `project`, `session_id`, `seq`, `assigned_to`, `notes`, `result` — Phase 후반. `parent_id`는 구현 완료 (1단계 sub-task, 자동 완료).
```

- [ ] **Step 2: §2.2 "모든 것은 작업 항목이다" 섹션에 sub-task 노트 추가**

`WorkType enum은 없다.` 문단 뒤에:

```markdown
**Sub-task:** `parent_id: Option<i64>`로 1단계 깊이의 부모-자식 관계를 표현한다. 모든 자식이 Done이면 부모가 자동으로 Done 전이. sub-task의 sub-task는 허용하지 않는다.
```

- [ ] **Step 3: Commit**

```bash
git add docs/v0.2/ARCHITECTURE.md
git commit -m "docs: update ARCHITECTURE.md with sub-task support"
```
