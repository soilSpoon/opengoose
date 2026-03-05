# Teams + Workflows 통합 및 아키텍처 개선 계획

## 현황 분석

Teams와 Workflows는 독립적으로 존재하지만, 공통 인프라를 공유하지 않아
코드 중복과 통합 불가 문제가 있음:

| 영역 | Teams | Workflows | 문제 |
|------|-------|-----------|------|
| 실행 추적 | `OrchestrationStore` (SQLite) | `WorkflowStore` (JSON 파일) | 이중 영속화 |
| 에이전트 | `ProfileStore` 참조 | 인라인 `AgentDef` | 프로필 재사용 불가 |
| 크래시 복구 | `suspend_incomplete()` + `!resume` | `save()/load()` + `resume_and_run()` | 통합 복구 없음 |
| 진입점 | `Engine.process_message()` | 별도 `WorkflowRunner` (게이트웨이 미연결) | 워크플로우 트리거 불가 |
| 파일명 안전화 | `TeamStore::path_for()` | `WorkflowStore::safe_filename()` | 중복 로직 |

---

## 변경 계획 (5단계)

### 1단계: 워크플로우 영속화를 Database로 마이그레이션

**변경 파일:**
- `crates/opengoose-persistence/migrations/` — `workflow_runs` 테이블 추가 마이그레이션
- `crates/opengoose-persistence/src/schema.rs` — 새 테이블 스키마
- `crates/opengoose-persistence/src/models.rs` — 새 모델
- `crates/opengoose-persistence/src/workflow_runs.rs` — 새 store (신규)
- `crates/opengoose-persistence/src/lib.rs` — 새 모듈 export
- `crates/opengoose-workflows/src/persist.rs` — `WorkflowStore`를 Database 기반으로 교체
- `crates/opengoose-workflows/Cargo.toml` — `opengoose-persistence` 의존성 추가

**workflow_runs 테이블:**
```sql
CREATE TABLE workflow_runs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_id       TEXT NOT NULL UNIQUE,
    session_key  TEXT,
    workflow_name TEXT NOT NULL,
    input        TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
    current_step INTEGER NOT NULL DEFAULT 0,
    total_steps  INTEGER NOT NULL DEFAULT 0,
    state_json   TEXT NOT NULL,  -- 전체 WorkflowState 직렬화
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- `state_json`은 기존 `WorkflowState`의 전체 JSON 직렬화본
- `status`/`current_step`/`total_steps`는 쿼리 편의를 위한 비정규화 컬럼
- SQLite WAL 모드 + 트랜잭션으로 원자적 쓰기 보장 (기존 temp+rename 불필요)

### 2단계: 워크플로우 에이전트가 ProfileStore 프로필 참조 가능하도록

**변경 파일:**
- `crates/opengoose-workflows/src/definition.rs` — `AgentDef`에 `profile` 필드 추가
- `crates/opengoose-workflows/src/engine.rs` — 프로필 로드 로직 추가 (옵션)
- `crates/opengoose-core/src/workflow_runner.rs` — `ProfileStore` 연동

**AgentDef 변경:**
```rust
pub struct AgentDef {
    pub id: String,
    pub name: String,
    /// 인라인 시스템 프롬프트 (profile과 상호 배타)
    #[serde(default)]
    pub system_prompt: String,
    /// ProfileStore의 프로필 이름 참조 (system_prompt 대신 사용 가능)
    #[serde(default)]
    pub profile: Option<String>,
}
```

YAML에서:
```yaml
agents:
  - id: architect
    name: Architect
    profile: researcher  # ProfileStore에서 로드
  - id: developer
    name: Developer
    system_prompt: "You are a senior developer..."  # 기존 인라인 방식도 유지
```

### 3단계: Engine에 워크플로우 실행 통합

**변경 파일:**
- `crates/opengoose-core/src/engine.rs` — 워크플로우 실행 메서드 추가

**추가 메서드:**
- `run_workflow(&self, session_key, workflow_name, input)` — 워크플로우 실행
- `list_workflows(&self)` — 사용 가능한 워크플로우 목록
- startup에서 번들 워크플로우 로드

이를 통해 Engine이 teams와 workflows 모두의 진입점이 됨.
`!workflow feature-dev "implement auth"` 같은 명령으로 TUI에서 트리거 가능.

### 4단계: 크래시 복구 통합

**변경 파일:**
- `crates/opengoose-core/src/engine.rs` — startup에서 workflow runs도 suspend

Engine 시작 시:
1. `OrchestrationStore.suspend_incomplete()` (기존 — teams)
2. `WorkflowRunStore.suspend_incomplete()` (신규 — workflows)
3. `!resume` 명령에서 teams와 workflows 모두 검색

### 5단계: 공유 유틸리티 추출

**변경 파일:**
- `crates/opengoose-types/src/lib.rs` — `sanitize_filename()` 함수 추가
- `crates/opengoose-teams/src/store.rs` — 공유 함수 사용
- `crates/opengoose-workflows/src/persist.rs` — 공유 함수 사용 (남은 용도가 있다면)

---

## 변경하지 않는 것

- **실행 엔진 자체는 분리 유지**: `WorkflowEngine`(순차 파이프라인)과
  `TeamOrchestrator`(Chain/FanOut/Router)는 근본적으로 다른 실행 모델이므로 합치지 않음
- **기존 YAML 형식 호환성 유지**: `system_prompt` 인라인 방식 계속 지원
- **기존 테스트 전부 유지**: 마이그레이션 후에도 45개 테스트 통과 보장
