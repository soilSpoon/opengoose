# WorkItem Sub-Task Support Design

> **작성일:** 2026-03-31
> **상태:** 승인됨

## 목표

WorkItem에 parent-child 관계를 추가하여 큰 작업을 sub-task로 분해하고, 각 sub-task를 독립적으로 Worker가 pull해서 처리할 수 있게 한다.

## 결정 사항

| 항목 | 결정 |
|------|------|
| 완료 규칙 | **자동 완료** — 모든 자식 Done → 부모 자동 Done |
| 생성 주체 | **누구나** — Worker, CLI, Operator 모두 가능 |
| 깊이 제한 | **1단계** — sub-task의 sub-task 불가 |
| 부모 상태 요건 | **Open/Claimed** — 둘 다 sub-task 생성 가능 |
| 실패 처리 | **자식 전부 Done일 때만** 부모 자동 완료. Abandoned 자식 있으면 부모 미완성 (수동 개입 필요) |

## 데이터 모델

### WorkItem 변경

```rust
pub struct WorkItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub created_by: RigId,
    pub created_at: DateTime<Utc>,
    pub status: Status,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub claimed_by: Option<RigId>,
    pub updated_at: DateTime<Utc>,
    pub parent_id: Option<i64>,  // NEW: None = top-level, Some(id) = sub-task
}
```

### PostWorkItem 변경

```rust
pub struct PostWorkItem {
    pub title: String,
    pub description: String,
    pub created_by: RigId,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub parent_id: Option<i64>,  // NEW
}
```

### SeaORM Entity 변경

`entity/work_item.rs`의 `Model`에 `parent_id: Option<i64>` 컬럼 추가.

### DB 마이그레이션

`Board::ensure_columns()`에 추가:
```sql
ALTER TABLE work_items ADD COLUMN parent_id INTEGER
```

기존 `ensure_columns` 패턴과 동일 — idempotent, "duplicate column" 에러 무시.

## Board API 변경

### `post()` — 깊이 검증 추가

`parent_id`가 Some이면:
1. parent가 존재하는지 확인 (`NotFound` 에러)
2. parent 자체가 sub-task가 아닌지 확인 (깊이 1단계 강제)
3. parent 상태가 Done/Abandoned이 아닌지 확인

새 에러 variant:
```rust
BoardError::MaxDepthExceeded { parent_id: i64 }
BoardError::ParentNotFound(i64)
BoardError::ParentCompleted { parent_id: i64 }
```

### `submit()` — 자동 완료 트리거

submit 성공 후:
1. 현재 item의 `parent_id` 확인
2. `parent_id`가 Some이면 → `children(parent_id)` 조회
3. 모든 자식이 `Status::Done`이면 → 부모를 `Done` 전이 (claimed_by는 유지)
4. 하나라도 Done이 아니면 → 아무것도 안 함

자동 완료는 `submit()` 내에서 실행. 별도 cron이나 polling 불필요.

### `children()` — 신규 쿼리

```rust
pub async fn children(&self, parent_id: i64) -> Result<Vec<WorkItem>, BoardError>
```

`parent_id` 컬럼으로 필터. 정렬은 `id ASC`.

### `ready()` — 변경 없음

sub-task도 일반 WorkItem과 동일하게 `Status::Open`이면 claim 가능. 부모-자식 관계는 ready 판정에 영향 없음.

### CowStore 동기화

`parent_id`는 CowStore의 `WorkItem`에도 포함. `insert_to_main`, `From<Model>` 변환에서 parent_id 매핑.

## CLI 변경

### `board create` — `--parent` 옵션 추가

```bash
opengoose board create "서브태스크 제목" --parent 5
```

`BoardAction::Create`에 `parent: Option<i64>` 필드 추가.

### `board children` — 신규 서브커맨드

```bash
opengoose board children 5
```

`BoardAction::Children { id: i64 }` variant 추가. 해당 작업의 sub-task 목록 출력.

### `board status` — 변경 없음

기존 출력 유지. sub-task도 일반 항목으로 카운트됨.

## MCP Tool 변경

### `create_task` 스키마

`parent_id` optional 필드 추가:
```json
{
  "parent_id": {"type": "integer", "description": "Parent work item ID for sub-tasks"}
}
```

### `create_task` 핸들러

`parent_id`가 있으면 `PostWorkItem { parent_id: Some(id), .. }`로 Board에 게시.

## Worker 동작

변경 없음. sub-task는 일반 WorkItem과 동일하게 Board에서 pull. 부모-자식 관계는 Board가 관리.

## Scope 밖

- TUI Board 탭에서 트리 형태 표시 (나중에)
- Web Dashboard API에서 parent/children 필터 (나중에)
- Beads prime_summary에서 sub-task 구조 표시 (나중에)
- 깊이 제한 해제 (나중에, 제한만 풀면 됨)
