# Experience Memory (Layer 2) Design

> **작성일:** 2026-04-01
> **상태:** 승인됨

## 목표

에이전트가 작업 중 발견한 지식을 저장하고, 다음 작업 시작 시 자동으로 주입하여 반복 실수를 줄인다.

## 결정 사항

| 항목 | 결정 |
|------|------|
| 범위 | Rig별 기본, 승격 가능 (rig → project → global) |
| 저장소 | SQLite (Board DB) — `memories` 테이블 |
| 검색 방식 | 전부 주입 (상위 10개, 시간 감쇠 가중치 순) |
| 주입 시점 | 작업 시작 시 자동 (MemoryHydrator middleware) |
| 저장 방식 | MCP tool `board__remember` |

## 데이터 모델

### memories 테이블

```sql
CREATE TABLE IF NOT EXISTS memories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rig_id TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'rig',
    content TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    last_used_at TIMESTAMP NOT NULL
);
```

- `rig_id`: 기억을 생성한 Worker의 RigId
- `scope`: `rig` (기본) | `project` | `global`
- `content`: 자유 텍스트 (에이전트가 작성)
- `last_used_at`: recall(주입)될 때마다 갱신 — 시간 감쇠 계산 기준

### Memory struct

```rust
pub struct Memory {
    pub id: i64,
    pub rig_id: RigId,
    pub scope: MemoryScope,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

pub enum MemoryScope {
    Rig,
    Project,
    Global,
}
```

### SeaORM Entity

`entity/memory.rs` — 기존 엔티티 패턴과 동일.

## 시간 감쇠

스킬 시스템과 동일한 30일 반감기:

```rust
fn memory_weight(memory: &Memory, now: DateTime<Utc>) -> f32 {
    let days = (now - memory.last_used_at).num_seconds() as f32 / 86400.0;
    0.5_f32.powf(days / 30.0)
}
```

## Board API

```rust
impl Board {
    /// 기억 저장. scope = Rig.
    pub async fn remember(&self, rig_id: &RigId, content: &str) -> Result<Memory, BoardError>;

    /// 기억 조회. 해당 rig의 rig-scope + project/global-scope 기억을 가중치 순으로 반환.
    /// 반환된 기억들의 last_used_at을 현재 시간으로 갱신.
    pub async fn recall(&self, rig_id: &RigId, limit: usize) -> Result<Vec<Memory>, BoardError>;

    /// 기억 승격. rig → project 또는 global.
    pub async fn promote_memory(&self, id: i64, scope: MemoryScope) -> Result<(), BoardError>;

    /// 전체 기억 목록 (관리용).
    pub async fn list_memories(&self, rig_id: Option<&RigId>) -> Result<Vec<Memory>, BoardError>;
}
```

### recall 로직

1. `WHERE rig_id = ? AND scope = 'rig'` UNION `WHERE scope IN ('project', 'global')` 조회
2. 각 기억에 `memory_weight()` 적용
3. 가중치 내림차순 정렬, 상위 `limit`개 반환
4. 반환된 기억들의 `last_used_at`을 `Utc::now()`로 UPDATE

last_used_at 갱신으로 자주 사용되는 기억은 자연스럽게 활성 상태를 유지하고, 사용되지 않는 기억은 감쇠된다.

## MCP Tool

### board__remember

에이전트가 작업 중 호출:

```json
{
  "cmd": "board__remember",
  "args": { "content": "이 프로젝트는 JWT로 인증하고, 토큰은 HttpOnly 쿠키에 저장한다" }
}
```

→ `Board.remember(rig_id, content)` 호출.

스키마:
```json
{
  "type": "object",
  "properties": {
    "content": { "type": "string", "description": "Knowledge to remember for future tasks" }
  },
  "required": ["content"]
}
```

## MemoryHydrator Middleware

새 middleware — `on_start`에서 자동 주입:

```rust
pub struct MemoryHydrator;

#[async_trait]
impl Middleware for MemoryHydrator {
    async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> {
        let memories = ctx.board.recall(ctx.rig_id, 10).await?;
        if memories.is_empty() {
            return Ok(());
        }
        let text = format_memories(&memories);
        ctx.agent.extend_system_prompt("memories".to_string(), text).await;
        Ok(())
    }
}
```

포맷:
```
## Memories (from previous tasks)
- 이 프로젝트는 JWT로 인증한다 (3일 전)
- CI는 nextest를 사용한다 (7일 전)
```

## 승격 (Web API)

스킬 승격과 동일한 패턴:

```
GET    /api/memories              — 전체 목록 (scope, age 포함)
POST   /api/memories/{id}/promote — { "scope": "project" }
```

## Runtime 와이어링

`init_runtime`에서 middleware 벡터에 `MemoryHydrator` 추가:

```rust
let middleware: Vec<Arc<dyn Middleware>> = vec![
    Arc::new(ContextHydrator { skill_catalog: String::new() }),
    Arc::new(MemoryHydrator),
    validation,
];
```

ContextHydrator 다음, ValidationGate 전에 배치.

## Scope 밖

- 기억 삭제 API (나중에)
- 기억 병합/요약 compact (나중에)
- 유사도 검색 / embedding (나중에)
- TUI에서 기억 표시 (나중에)
- 기억 개수 상한 (현재 시간 감쇠로 자연 관리)
