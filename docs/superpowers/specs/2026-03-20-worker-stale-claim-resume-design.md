# Worker Stale Claim Resume

## 배경

Worker가 work item을 claim한 뒤 크래시/종료되면, 해당 아이템이 `Claimed` 상태로 DB에 남는다. `board.ready()`는 `Status::Open`만 반환하므로 이 아이템들은 영원히 처리되지 않는다.

## 결정사항

1. **Resume, not unclaim**: 같은 rig ID로 재시작 시 자기가 claim한 아이템을 바로 재처리한다. unclaim → re-claim은 레이스 컨디션 위험 + 불필요한 상태 전이.
2. **즉시 resume**: 시간 경과 등 조건 없이, `claimed_by = self.id`인 아이템 전부 resume.
3. **세션 연속성**: goose의 `create_session`은 매번 새 ID를 생성한다. resume 시에는 `list_sessions()`로 기존 `"task-{item.id}"` name의 세션을 찾아 그 session ID를 재사용한다. 기존 세션이 없으면 새로 생성한다.
4. **정렬**: `board.ready()`와 동일하게 priority 내림차순.
5. **에러 처리**: 기존 pull loop과 동일 — 실패 시 `board.abandon()`, 다음 아이템으로 continue.

## 변경사항

### 1. Board API — `claimed_by()` 메서드 추가

**파일**: `crates/opengoose-board/src/board.rs`

```rust
pub async fn claimed_by(&self, rig_id: &RigId) -> Result<Vec<WorkItem>, BoardError> {
    // status = Claimed AND claimed_by = rig_id
    // ORDER BY priority DESC
}
```

- SeaORM 쿼리로 `Status::Claimed` + `claimed_by` 필터
- priority 내림차순 정렬
- blocked 아이템 제외 불필요 — 이미 claim된 상태

### 2. `process_claimed_item()` 추출

**파일**: `crates/opengoose-rig/src/rig.rs`

현재 `try_claim_and_execute()` 내부의 claim 이후 로직(세션 생성 → process → submit/abandon)을 별도 메서드로 추출한다.

```rust
/// 에러를 내부에서 처리 (submit or abandon). 호출자에게 전파하지 않음.
async fn process_claimed_item(&self, item: &WorkItem, board: &Board) {
    // 1. 세션 조회 or 생성
    let session_name = format!("task-{}", item.id);
    let sessions = session_manager.list_sessions().await.unwrap_or_default();
    let session = sessions.iter().find(|s| s.name == session_name);
    let session_id = match session {
        Some(s) => s.id.clone(),           // 기존 세션 재사용 (히스토리 유지)
        None => create_session(...).id,     // 새 세션 생성
    };

    // 2. WorkInput 생성 + process
    let input = WorkInput::task(...).with_session_id(session_id);
    let result = self.process(input).await;

    // 3. submit or abandon
    match result {
        Ok(()) => { board.submit(item.id, &self.id).await.ok(); }
        Err(e) => {
            warn!(..., "execution failed, abandoning");
            board.abandon(item.id).await.ok();
        }
    }
}
```

- 에러를 삼킴 — submit/abandon 실패도 `.ok()`로 무시. resume 루프와 pull 루프 모두 개별 아이템 실패가 전체를 중단시키지 않음.
- `try_claim_and_execute()`도 이 메서드를 호출하되, 반환값을 `Ok(true)`로 매핑하여 기존 pull loop 흐름 유지.
- 세션 조회: `session_manager.list_sessions()`에서 name이 `"task-{item.id}"`인 마지막 세션을 찾음. 있으면 재사용 (히스토리 유지), 없으면 새로 생성.

### 3. Worker::run() — resume 단계 추가

**파일**: `crates/opengoose-rig/src/rig.rs`

현재 `run()`은 바로 pull loop에 진입한다. resume 단계를 앞에 추가:

```rust
pub async fn run(&self) {
    let Some(board) = &self.board else { return; };

    // Phase 1: Resume — 이전에 claim한 아이템 처리
    let stale = board.claimed_by(&self.id).await.unwrap_or_default();
    if !stale.is_empty() {
        info!(rig = %self.id, count = stale.len(), "resuming previously claimed items");
    }
    for item in stale {
        if self.cancel.is_cancelled() { break; }
        self.process_claimed_item(&item, board).await;
        // 에러 발생해도 다음 아이템으로 continue (abandon 내부 처리)
    }

    // Phase 2: Pull loop (기존 코드, 변경 없음)
    loop { ... }
}
```

## 변경 범위

| 파일 | 변경 내용 |
|------|-----------|
| `crates/opengoose-board/src/board.rs` | `claimed_by()` 메서드 추가 |
| `crates/opengoose-rig/src/rig.rs` | `process_claimed_item()` 추출, `run()` 시작부에 resume 루프 추가 |

새 파일 없음.
