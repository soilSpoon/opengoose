# Experience Memory (Layer 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 에이전트가 작업 중 발견한 지식을 저장(`board__remember`)하고, 다음 작업 시 자동 주입(MemoryHydrator middleware)하여 반복 실수를 줄인다.

**Architecture:** `memories` SQLite 테이블 + SeaORM entity. Board에 `remember`/`recall`/`promote_memory` API 추가. `MemoryHydrator` middleware가 작업 시작 시 상위 10개 기억을 시간 감쇠(30일 반감기) 순으로 시스템 프롬프트에 주입. MCP tool `board__remember`로 에이전트가 저장. Web API `/api/memories`로 관리.

**Tech Stack:** Rust, SeaORM, SQLite, axum

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `crates/opengoose-board/src/entity/memory.rs` | SeaORM entity — memories 테이블 | Create |
| `crates/opengoose-board/src/entity/mod.rs` | Entity 모듈 등록 | Modify |
| `crates/opengoose-board/src/memory.rs` | Memory struct + MemoryScope + Board API | Create |
| `crates/opengoose-board/src/board.rs` | create_tables에 memories 추가 | Modify |
| `crates/opengoose-board/src/lib.rs` | memory 모듈 + re-export | Modify |
| `crates/opengoose-rig/src/pipeline.rs` | MemoryHydrator middleware | Modify |
| `crates/opengoose-rig/src/mcp_tools/schema.rs` | board__remember tool 스키마 | Modify |
| `crates/opengoose-rig/src/mcp_tools/handlers.rs` | board__remember 핸들러 | Modify |
| `crates/opengoose/src/web/api/memories.rs` | Web API /api/memories | Create |
| `crates/opengoose/src/web/api/mod.rs` | memories 모듈 등록 | Modify |
| `crates/opengoose/src/web/mod.rs` | 라우터에 /api/memories 추가 | Modify |
| `crates/opengoose/src/runtime.rs` | middleware에 MemoryHydrator 추가 | Modify |

---

### Task 1: SeaORM Entity + Memory struct + Board API

**Files:**
- Create: `crates/opengoose-board/src/entity/memory.rs`
- Create: `crates/opengoose-board/src/memory.rs`
- Modify: `crates/opengoose-board/src/entity/mod.rs`
- Modify: `crates/opengoose-board/src/board.rs`
- Modify: `crates/opengoose-board/src/lib.rs`

- [ ] **Step 1: Entity 생성**

Create `crates/opengoose-board/src/entity/memory.rs`:

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "memories")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub rig_id: String,
    pub scope: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

Register in `crates/opengoose-board/src/entity/mod.rs`:

```rust
pub mod memory;
```

- [ ] **Step 2: Memory struct + MemoryScope + Board API**

Create `crates/opengoose-board/src/memory.rs`:

```rust
//! Experience Memory — 에이전트 경험 기억 저장 및 조회.

use crate::board::{Board, db_err};
use crate::entity;
use crate::work_item::{BoardError, RigId};
use chrono::{DateTime, Utc};
use sea_orm::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MemoryScope {
    Rig,
    Project,
    Global,
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryScope::Rig => "rig",
            MemoryScope::Project => "project",
            MemoryScope::Global => "global",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rig" => Some(MemoryScope::Rig),
            "project" => Some(MemoryScope::Project),
            "global" => Some(MemoryScope::Global),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Memory {
    pub id: i64,
    pub rig_id: RigId,
    pub scope: MemoryScope,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

impl From<entity::memory::Model> for Memory {
    fn from(m: entity::memory::Model) -> Self {
        Memory {
            id: m.id,
            rig_id: RigId::new(m.rig_id),
            scope: MemoryScope::parse(&m.scope).unwrap_or(MemoryScope::Rig),
            content: m.content,
            created_at: m.created_at,
            last_used_at: m.last_used_at,
        }
    }
}

/// 시간 감쇠 가중치. 30일 반감기.
pub fn memory_weight(memory: &Memory, now: DateTime<Utc>) -> f32 {
    let days = (now - memory.last_used_at).num_seconds() as f32 / 86400.0;
    0.5_f32.powf(days / 30.0)
}

impl Board {
    /// 기억 저장. scope = Rig.
    pub async fn remember(&self, rig_id: &RigId, content: &str) -> Result<Memory, BoardError> {
        let now = Utc::now();
        let result = entity::memory::Entity::insert(entity::memory::ActiveModel {
            id: NotSet,
            rig_id: Set(rig_id.0.clone()),
            scope: Set("rig".to_string()),
            content: Set(content.to_string()),
            created_at: Set(now),
            last_used_at: Set(now),
        })
        .exec(&self.db)
        .await
        .map_err(db_err)?;

        let model = entity::memory::Entity::find_by_id(result.last_insert_id)
            .one(&self.db)
            .await
            .map_err(db_err)?
            .ok_or(BoardError::DbError("memory insert failed".into()))?;

        Ok(Memory::from(model))
    }

    /// 기억 조회. rig-scope + project/global-scope. 시간 감쇠 가중치 순. last_used_at 갱신.
    pub async fn recall(
        &self,
        rig_id: &RigId,
        limit: usize,
    ) -> Result<Vec<Memory>, BoardError> {
        // Fetch: this rig's memories + project/global scope
        let models = entity::memory::Entity::find()
            .filter(
                Condition::any()
                    .add(entity::memory::Column::RigId.eq(rig_id.as_ref())
                        .and(entity::memory::Column::Scope.eq("rig")))
                    .add(entity::memory::Column::Scope.is_in(["project", "global"])),
            )
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let now = Utc::now();
        let mut memories: Vec<Memory> = models.into_iter().map(Memory::from).collect();
        memories.sort_by(|a, b| {
            memory_weight(b, now)
                .partial_cmp(&memory_weight(a, now))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        memories.truncate(limit);

        // Update last_used_at for returned memories
        if !memories.is_empty() {
            let ids: Vec<i64> = memories.iter().map(|m| m.id).collect();
            entity::memory::Entity::update_many()
                .col_expr(entity::memory::Column::LastUsedAt, Expr::value(now))
                .filter(entity::memory::Column::Id.is_in(ids))
                .exec(&self.db)
                .await
                .map_err(db_err)?;
        }

        Ok(memories)
    }

    /// 기억 승격. rig → project 또는 global.
    pub async fn promote_memory(
        &self,
        id: i64,
        scope: MemoryScope,
    ) -> Result<(), BoardError> {
        let model = entity::memory::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(db_err)?
            .ok_or(BoardError::NotFound(id))?;

        let mut active: entity::memory::ActiveModel = model.into();
        active.scope = Set(scope.as_str().to_string());
        active.update(&self.db).await.map_err(db_err)?;
        Ok(())
    }

    /// 전체 기억 목록 (관리용). rig_id 필터 optional.
    pub async fn list_memories(
        &self,
        rig_id: Option<&RigId>,
    ) -> Result<Vec<Memory>, BoardError> {
        let mut query = entity::memory::Entity::find();
        if let Some(rid) = rig_id {
            query = query.filter(entity::memory::Column::RigId.eq(rid.as_ref()));
        }
        query
            .order_by_desc(entity::memory::Column::LastUsedAt)
            .all(&self.db)
            .await
            .map(|models| models.into_iter().map(Memory::from).collect())
            .map_err(db_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_scope_roundtrip() {
        assert_eq!(MemoryScope::parse("rig"), Some(MemoryScope::Rig));
        assert_eq!(MemoryScope::parse("project"), Some(MemoryScope::Project));
        assert_eq!(MemoryScope::parse("global"), Some(MemoryScope::Global));
        assert_eq!(MemoryScope::parse("invalid"), None);
        assert_eq!(MemoryScope::Rig.as_str(), "rig");
    }

    #[test]
    fn memory_weight_decays() {
        let now = Utc::now();
        let fresh = Memory {
            id: 1,
            rig_id: RigId::new("w"),
            scope: MemoryScope::Rig,
            content: "test".into(),
            created_at: now,
            last_used_at: now,
        };
        let old = Memory {
            last_used_at: now - chrono::Duration::days(30),
            ..fresh.clone()
        };
        let w_fresh = memory_weight(&fresh, now);
        let w_old = memory_weight(&old, now);
        assert!(w_fresh > w_old, "fresh={w_fresh} should be > old={w_old}");
        assert!((w_fresh - 1.0).abs() < 0.01, "fresh weight should be ~1.0");
        assert!((w_old - 0.5).abs() < 0.01, "30-day old weight should be ~0.5");
    }

    #[tokio::test]
    async fn remember_and_recall() {
        let board = Board::in_memory().await.expect("board");
        let rig = RigId::new("worker-1");

        let m = board.remember(&rig, "JWT auth").await.expect("remember");
        assert_eq!(m.content, "JWT auth");
        assert_eq!(m.scope, MemoryScope::Rig);

        let recalled = board.recall(&rig, 10).await.expect("recall");
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, "JWT auth");
    }

    #[tokio::test]
    async fn recall_includes_project_and_global() {
        let board = Board::in_memory().await.expect("board");
        let rig = RigId::new("worker-1");
        let other = RigId::new("worker-2");

        board.remember(&rig, "rig memory").await.expect("remember");
        board.remember(&other, "other rig").await.expect("remember");

        // Promote other's memory to project scope
        let other_memories = board.list_memories(Some(&other)).await.expect("list");
        board
            .promote_memory(other_memories[0].id, MemoryScope::Project)
            .await
            .expect("promote");

        // rig should see own rig-scope + project-scope (but not other's rig-scope)
        let recalled = board.recall(&rig, 10).await.expect("recall");
        assert_eq!(recalled.len(), 2); // own rig + promoted project
    }

    #[tokio::test]
    async fn recall_respects_limit() {
        let board = Board::in_memory().await.expect("board");
        let rig = RigId::new("w");
        for i in 0..20 {
            board
                .remember(&rig, &format!("memory {i}"))
                .await
                .expect("remember");
        }
        let recalled = board.recall(&rig, 5).await.expect("recall");
        assert_eq!(recalled.len(), 5);
    }

    #[tokio::test]
    async fn promote_memory_changes_scope() {
        let board = Board::in_memory().await.expect("board");
        let rig = RigId::new("w");
        let m = board.remember(&rig, "promote me").await.expect("remember");
        board
            .promote_memory(m.id, MemoryScope::Global)
            .await
            .expect("promote");
        let memories = board.list_memories(None).await.expect("list");
        assert_eq!(memories[0].scope, MemoryScope::Global);
    }

    #[tokio::test]
    async fn list_memories_filters_by_rig() {
        let board = Board::in_memory().await.expect("board");
        board.remember(&RigId::new("a"), "a-mem").await.expect("remember");
        board.remember(&RigId::new("b"), "b-mem").await.expect("remember");

        let all = board.list_memories(None).await.expect("list");
        assert_eq!(all.len(), 2);

        let a_only = board.list_memories(Some(&RigId::new("a"))).await.expect("list");
        assert_eq!(a_only.len(), 1);
        assert_eq!(a_only[0].content, "a-mem");
    }
}
```

- [ ] **Step 3: Board create_tables에 memories 추가**

`crates/opengoose-board/src/board.rs`의 `create_tables`에 추가:

```rust
schema.create_table_from_entity(entity::memory::Entity),
```

기존 `create_table_from_entity` 목록의 마지막에 추가.

- [ ] **Step 4: lib.rs에 memory 모듈 + re-export**

`crates/opengoose-board/src/lib.rs`에 추가:

```rust
pub mod memory;
```

그리고 re-export:

```rust
pub use memory::{Memory, MemoryScope};
```

- [ ] **Step 5: cargo check + cargo test -p opengoose-board 통과 확인**

Run: `cargo check && cargo test -p opengoose-board`
Expected: 전부 PASS

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-board/
git commit -m "feat(board): add experience memory — remember/recall/promote API"
```

---

### Task 2: MemoryHydrator Middleware

**Files:**
- Modify: `crates/opengoose-rig/src/pipeline.rs`
- Modify: `crates/opengoose/src/runtime.rs`

- [ ] **Step 1: MemoryHydrator 구현**

`crates/opengoose-rig/src/pipeline.rs`에 추가 (ValidationGate 뒤):

```rust
/// Injects top-N memories into the system prompt on task start.
///
/// Fetches the worker's rig-scope memories plus project/global memories,
/// ranked by time-decay weight (30-day half-life), and injects them
/// as a `## Memories` section.
pub struct MemoryHydrator;

#[async_trait::async_trait]
impl Middleware for MemoryHydrator {
    async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> {
        let memories = ctx.board.recall(ctx.rig_id, 10).await?;
        if memories.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now();
        let mut lines = vec!["## Memories (from previous tasks)".to_string()];
        for m in &memories {
            let age = now - m.last_used_at;
            let age_str = if age.num_days() > 0 {
                format!("{}일 전", age.num_days())
            } else if age.num_hours() > 0 {
                format!("{}시간 전", age.num_hours())
            } else {
                "방금".to_string()
            };
            lines.push(format!("- {} ({})", m.content, age_str));
        }
        let text = lines.join("\n");
        ctx.agent
            .extend_system_prompt("memories".to_string(), text)
            .await;
        Ok(())
    }
}
```

- [ ] **Step 2: runtime.rs에서 middleware 벡터에 MemoryHydrator 추가**

`crates/opengoose/src/runtime.rs`의 middleware 벡터에 추가:

```rust
use opengoose_rig::pipeline::{ContextHydrator, MemoryHydrator, Middleware, ValidationGate};

let middleware: Vec<Arc<dyn Middleware>> = vec![
    Arc::new(ContextHydrator { skill_catalog: String::new() }),
    Arc::new(MemoryHydrator),
    validation,
];
```

ContextHydrator 다음, ValidationGate 전에 배치.

- [ ] **Step 3: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/pipeline.rs crates/opengoose/src/runtime.rs
git commit -m "feat(rig): add MemoryHydrator middleware for auto-injection"
```

---

### Task 3: MCP Tool — board__remember

**Files:**
- Modify: `crates/opengoose-rig/src/mcp_tools/schema.rs`
- Modify: `crates/opengoose-rig/src/mcp_tools/handlers.rs`

- [ ] **Step 1: 테스트 작성**

`crates/opengoose-rig/src/mcp_tools/handlers.rs` tests 모듈에 추가:

```rust
#[tokio::test]
async fn remember_stores_memory() {
    let board = Arc::new(Board::in_memory().await.expect("board"));
    let rig_id = RigId::new("test-rig");
    let mut args = JsonObject::new();
    args.insert("content".into(), json!("JWT auth with HttpOnly cookies"));
    let result = handle_remember(&board, &rig_id, &args).await;
    let text = content_text(&result);
    assert!(text.contains("Remembered"));

    let memories = board.list_memories(Some(&rig_id)).await.expect("list");
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].content, "JWT auth with HttpOnly cookies");
}

#[tokio::test]
async fn remember_missing_content_returns_error() {
    let board = Arc::new(Board::in_memory().await.expect("board"));
    let rig_id = RigId::new("test-rig");
    let args = JsonObject::new();
    let result = handle_remember(&board, &rig_id, &args).await;
    let text = content_text(&result);
    assert!(text.contains("Missing content"));
}
```

- [ ] **Step 2: schema.rs에 board__remember tool 추가**

`crates/opengoose-rig/src/mcp_tools/schema.rs`의 `board_tools()` 벡터에 추가:

```rust
tool_def(
    "board__remember",
    "Store a piece of knowledge learned during this task for future reference.",
    serde_json::json!({
        "type": "object",
        "properties": {
            "content": {"type": "string", "description": "Knowledge to remember for future tasks"}
        },
        "required": ["content"]
    }),
),
```

`board_tools_returns_four` 테스트를 `board_tools_returns_five`로 변경하고 assert를 `5`로.

- [ ] **Step 3: handlers.rs에 handle_remember 구현**

`crates/opengoose-rig/src/mcp_tools/handlers.rs`에 추가:

```rust
pub async fn handle_remember(
    board: &Arc<Board>,
    rig_id: &RigId,
    args: &JsonObject,
) -> CallToolResult {
    let Some(content) = args.get("content").and_then(Value::as_str) else {
        return CallToolResult::error(vec![Content::text("Missing content")]);
    };

    match board.remember(rig_id, content).await {
        Ok(m) => CallToolResult::success(vec![Content::text(format!(
            "Remembered: \"{}\" (id: {})",
            m.content, m.id
        ))]),
        Err(e) => CallToolResult::error(vec![Content::text(format!("Remember failed: {e}"))]),
    }
}
```

Also update the MCP tool dispatch (in `crates/opengoose-rig/src/mcp_tools/mod.rs` or wherever tools are dispatched) to route `board__remember` to `handle_remember`. Read the file first to find the dispatch location.

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p opengoose-rig remember`
Expected: 2 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-rig/src/mcp_tools/
git commit -m "feat(mcp): add board__remember tool for experience memory"
```

---

### Task 4: Web API — /api/memories

**Files:**
- Create: `crates/opengoose/src/web/api/memories.rs`
- Modify: `crates/opengoose/src/web/api/mod.rs`
- Modify: `crates/opengoose/src/web/mod.rs`

- [ ] **Step 1: memories.rs API 핸들러 작성**

Create `crates/opengoose/src/web/api/memories.rs`:

```rust
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use opengoose_board::memory::MemoryScope;
use opengoose_board::Memory;
use serde::Deserialize;

use super::AppState;

pub async fn memories_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<Memory>>, StatusCode> {
    state
        .board
        .list_memories(None)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
pub struct PromoteRequest {
    pub scope: String,
}

pub async fn memories_promote(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<PromoteRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let scope = MemoryScope::parse(&body.scope).ok_or((
        StatusCode::BAD_REQUEST,
        format!("invalid scope: {}", body.scope),
    ))?;
    state
        .board
        .promote_memory(id, scope)
        .await
        .map(|_| StatusCode::OK)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Promote failed: {e}")))
}
```

- [ ] **Step 2: api/mod.rs에 등록**

```rust
mod memories;
pub use memories::{memories_list, memories_promote};
```

- [ ] **Step 3: web/mod.rs에 라우트 추가**

```rust
.route("/api/memories", axum::routing::get(api::memories_list))
.route("/api/memories/{id}/promote", axum::routing::post(api::memories_promote))
```

- [ ] **Step 4: cargo check 통과 확인**

Run: `cargo check`
Expected: 성공

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/web/
git commit -m "feat(web): add /api/memories endpoints for listing and promotion"
```

---

### Task 5: ARCHITECTURE.md 업데이트

**Files:**
- Modify: `docs/v0.2/ARCHITECTURE.md`

- [ ] **Step 1: §14 열린 질문 6번 업데이트**

기존:
```
6. **경험 기억 (Layer 2)?** 설계됨 (원본 ARCHITECTURE.md § 4.5) 하지만 미구현. `board__remember`/`board__recall` 도구, 시간 감쇠, pre-compaction flush 등.
```

변경:
```
6. ~~**경험 기억 (Layer 2)?**~~ **해결됨.** `board__remember` MCP tool + `MemoryHydrator` middleware. Rig별 기억, project/global 승격 가능. 30일 반감기 시간 감쇠. `/api/memories`로 관리.
```

- [ ] **Step 2: Commit**

```bash
git add docs/v0.2/ARCHITECTURE.md
git commit -m "docs: update ARCHITECTURE.md with experience memory"
```
