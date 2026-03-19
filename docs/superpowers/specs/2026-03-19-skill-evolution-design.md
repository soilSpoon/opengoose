# Skill Evolution System — Design Spec

## Problem

OpenGoose v0.2에서 에이전트가 작업을 완료하면 그 경험이 사라진다.
MetaClaw/AutoResearchClaw 리서치 결과 "경험 → 스킬 파일 → 다음 세션에 주입" 패턴이 가장 실용적.
Wasteland stamp 시스템을 트리거로 활용: 낮은 stamp → LLM이 교훈 추출 → SKILL.md 저장.

현재 구현(v0.2 Phase 2)은 템플릿 기반으로 고정 가이드라인을 출력하며,
Claude Skill Creator와 MetaClaw의 핵심 원칙이 반영되지 않았다.

## Goals

1. Board를 DbBoard로 통일하고 in-memory Board 제거
2. Evolver 전용 rig으로 LLM 기반 스킬 자동 생성
3. Claude Skill Creator + MetaClaw 원칙을 반영한 스킬 품질
4. 3단계 스코프 (Global / Project / Rig) + 출처 2종류 (installed / learned)
5. 스킬 lifecycle (Active → Dormant → Archived)

## Non-Goals

- 범용 Worker pull loop 가동 (별도 작업)
- 스킬 승격 UI (이번엔 디렉토리 구조만)
- RL/파인튜닝 기반 학습

---

## Architecture

### 1. Board 통일

DbBoard를 `Board`로 rename. in-memory Board + CowStore + branch + merge 제거.

**제거 대상:**
- `crates/opengoose-board/src/board.rs` — in-memory Board
- `crates/opengoose-board/src/store.rs` — CowStore
- `crates/opengoose-board/src/branch.rs`
- `crates/opengoose-board/src/merge.rs`

**변경:**
- `db_board.rs` → `board.rs`, `DbBoard` → `Board`
- `lib.rs` — re-export 정리, Board/CowStore 등 제거된 모듈 정리
- `Rig<M>` — `board: Option<Arc<Mutex<Board>>>` → `Option<Arc<Board>>` (Mutex 불필요, async + 커넥션풀)
- `BoardClient` (mcp_tools.rs) — `Arc<Mutex<Board>>` → `Arc<Board>`. 모든 `.lock().await` + sync 호출을 async 호출로 변경 (read_board, claim_next, submit, create_task 전부)
- `Worker::run()` / `try_claim_and_execute()` — `board.lock().await` 제거, async Board 메서드 직접 호출
- 테스트 — `Board::in_memory()` 사용 (기존 `DbBoard::in_memory()`)

### 2. System Rig

자동 등록, 삭제/수정 불가한 system rig.

```
rigs 테이블:
  id: "human"    type: "system"   — CLI stamp의 stamped_by
  id: "evolver"  type: "system"   — Evolver rig
  id: "worker-1" type: "ai"       — 사용자 등록 (삭제 가능)
```

- `Board::connect()` 시 `ensure_system_rigs()` 호출하여 자동 등록
- `remove_rig()` — `rig_type == "system"`이면 `BoardError::SystemRigProtected` 반환 (work_item.rs에 variant 추가)

**CLI 변경:**
- `opengoose board stamp` — `--by` 옵션 제거, `stamped_by = "human"` 자동
- `opengoose rigs remove human` → Error: cannot remove system rig

### 3. stamp_notify + Evolver

**Board 변경:**
```rust
pub struct Board {
    db: DatabaseConnection,
    notify: Arc<Notify>,        // 작업 claimable 시
    stamp_notify: Arc<Notify>,  // stamp 추가 시 (NEW)
}
```

- `add_stamp()` 끝에 `self.stamp_notify.notify_waiters()` 추가
- `pub fn stamp_notify_handle(&self) -> Arc<Notify>` 추가

**EvolveMode:**
```rust
pub struct EvolveMode;

impl WorkMode for EvolveMode {
    fn session_for(&self, input: &WorkInput) -> String {
        format!("evolve-{}", input.work_id.unwrap_or(0))
    }
}

pub type Evolver = Rig<EvolveMode>;
```

**stamp 처리 추적:**

stamp entity에 `evolved_at: Option<DateTime<Utc>>` 컬럼 추가.
Evolver가 해당 stamp에 대해 work item을 생성할 때 즉시 `evolved_at = now()` 설정.
이렇게 하면:
- Evolver가 미처리 stamp을 `WHERE score < 0.3 AND evolved_at IS NULL`로 조회 가능
- 복수 Evolver 시 첫 번째가 `evolved_at`을 설정하면 다른 Evolver는 조회 결과에서 제외 (중복 방지)
- `stamp_notify`는 hint일 뿐, 정확성은 `evolved_at` 쿼리가 보장

**low stamp 임계값:** score < 0.3 (기본값). 추후 설정 가능하게 확장 가능.

**Evolver 루프:**
1. `stamp_notify.notified()` 대기 (+ 5분 주기 폴백 sweep)
2. `WHERE score < 0.3 AND evolved_at IS NULL` 조회
3. 각 stamp에 대해:
   a. `evolved_at = now()` 설정 (원자적, 다른 Evolver의 중복 처리 방지)
   b. Board에 "스킬 생성" work item self-post (`created_by: "evolver"`)
   c. claim → LLM 분석 (agent.reply) → SKILL.md 생성 → submit
   d. 실패 시 work item을 stuck으로, `evolved_at` 유지 (재처리 안 함)
4. 다시 1로

**Notify는 hint:** `tokio::sync::Notify`는 대기 중인 task만 깨움. Evolver가 처리 중일 때 도착한 stamp은 notify가 유실되지만, 5분 폴백 sweep이 커버. 정확성은 `evolved_at IS NULL` 쿼리가 보장.

**Lazy init:** main.rs에서 stamp_notify listener만 즉시 spawn. 첫 stamp 이벤트가 올 때 Agent 생성.

**복수 Evolver:** system rig "evolver"는 최소 1개 유지. 추가 Evolver는 `opengoose rigs add --id evolver-2 --recipe evolver`로 등록. 모든 Evolver가 같은 쿼리(`evolved_at IS NULL`)를 사용하되, `evolved_at` 설정이 원자적이라 중복 처리 방지.

**Evolver의 work item stamping:** Evolver가 만든 "스킬 생성" work item은 사람이 결과물(SKILL.md)을 확인 후 stamp. Evolver의 trust level은 이 stamp으로 결정됨.

**대화 로그 접근:** Evolver는 work_item_id로부터 `session_id = format!("task-{}", work_item_id)`를 유도하여 `~/.opengoose/logs/task-{id}.jsonl`을 읽음. 로그가 없으면 stamp comment + work item 정보만으로 분석 (fallback).

**main.rs 와이어링:**
```rust
let board = Arc::new(Board::connect(&db_url()).await?);
let stamp_notify = board.stamp_notify_handle();

// Evolver: lazy init, stamp_notify listen
tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));

// Operator: TUI
let (agent, session_id) = create_agent().await?;
tui::run_tui(board, agent, session_id).await
```

### 4. LLM 기반 스킬 생성

**Evolver 시스템 프롬프트:**
```
You are a skill analyst for OpenGoose.
Analyze failed tasks and extract concrete, actionable lessons as SKILL.md files.

Rules:
- description MUST start with "Use when..." (triggering conditions only)
- description must NOT summarize the skill's workflow
- Every lesson must be specific to THIS failure, not generic advice
- Include a "Common Mistakes" table with specific rationalizations
- Include a "Red Flags" list for self-checking
- If the lesson is something any competent agent already knows, output SKIP
- If an existing skill covers the same lesson, output UPDATE:{skill-name}

Before creating, check existing skills:
[기존 스킬 name+desc 목록이 여기에 주입됨]
```

**agent.reply() 입력:**
```
Analyze this failed task and create a SKILL.md.

## Stamp
dimension: {dimension}, score: {score}, comment: '{comment}'

## Work Item
#{id}: '{title}'

## Conversation Log
{대화 로그 요약 — 최대 2000 tokens}

Generate a SKILL.md with YAML frontmatter (name, description) and markdown body.
Or output SKIP if the lesson is too generic.
Or output UPDATE:{name} if an existing skill should be updated instead.
```

**LLM 출력 검증 (Claude Skill Creator 참고):**
1. `---` frontmatter 존재 여부
2. `name:` — lowercase + hyphens only, 1-64자
3. `description:` — "Use when"으로 시작, 1024자 이내
4. 검증 실패 → 1회 재시도 (프롬프트에 "fix the format" 추가)
5. 2회 실패 → skip, work item을 stuck으로, 로그 남김

**스킬 효과 추적 (MetaClaw 참고):**
```json
// metadata.json
{
  "generated_from": {
    "stamp_id": 5,
    "work_item_id": 42,
    "dimension": "Quality",
    "score": 0.2
  },
  "generated_at": "2026-03-19T10:00:00Z",
  "evolver_work_item_id": 100,
  "effectiveness": {
    "injected_count": 0,
    "subsequent_scores": []
  }
}
```

스킬이 catalog에 포함된 후, 같은 rig의 같은 dimension stamp score를 `subsequent_scores`에 추가.

**효과 판정 규칙:**
- `subsequent_scores`가 3개 이상 쌓이면 판정 시작
- 평균이 생성 시 score보다 0.2 이상 개선 → 효과 있음 (유지)
- 평균이 개선 없거나 악화 → 효과 없음 (decay 가속)
- 스킬 UPDATE 시 `subsequent_scores` 리셋

### 5. 스킬 스코프 + 디렉토리 구조

**3단계 스코프:**

```
Global:  ~/.opengoose/skills/
           ├── installed/     — 수동 설치, 모든 rig/프로젝트 공유
           └── learned/       — 승격된 자동 생성 스킬

Project: {cwd}/.opengoose/skills/
           ├── installed/     — 프로젝트별 수동 설치
           └── learned/       — 승격된 자동 생성 스킬

Rig:     ~/.opengoose/rigs/{rig-id}/skills/
           └── learned/       — Evolver 기본 생성 위치 (target rig 기준)
```

**출처 2종류:**
- `installed/` — 수동 설치 (`opengoose skills add`). decay 대상 아님.
- `learned/` — Evolver 자동 생성. lifecycle 관리 대상.

**Evolver 생성 위치:** stamp의 target rig 디렉토리.
예: `worker-1`이 Quality 낮은 stamp 받으면 → `~/.opengoose/rigs/worker-1/skills/learned/test-before-submit/`

**catalog 로딩 순서 (구체적 → 일반적):**
1. Rig-specific (해당 rig에만 적용)
2. Project (이 프로젝트의 모든 rig)
3. Global (모든 프로젝트의 모든 rig)

중복 name이면 더 구체적인 스코프가 우선.

**승격 (이번엔 디렉토리 구조만, UI는 나중에):**
- rig → project: 파일 복사
- rig → global: 파일 복사
- project → global: 파일 복사

### 6. 스킬 Lifecycle

**3단계 lifecycle (learned 스킬에만 적용):**

```
Active
  │ 조건: 30일 이내 생성 OR effectiveness 양호
  │ catalog에 name+desc 주입
  │
  ▼ 30일 미참조 OR effectiveness 나쁨

Dormant
  │ catalog에서 제외 (토큰 절약)
  │ skills list에 (dormant) 표시
  │ 사람이 수동으로 Active 복원 가능
  │
  ▼ 90일 dormant 유지

Archived
    learned/ → archived/ 디렉토리로 이동
    skills list --archived로 확인 가능
    사람이 복원 가능
```

**참조 추적:** `metadata.json`의 `last_included_at` 타임스탬프.
`build_catalog()`가 스킬을 catalog에 포함할 때마다 갱신.

**catalog 상한:**
- 최대 10개 스킬 (installed + learned Active)
- 총 ~500 words (name + description만)
- 10개 초과 시 learned부터 제외 (installed 우선)
- effectiveness score 높은 순으로 정렬

### 7. CLI 변경 요약

```bash
# stamp — --by 제거
opengoose board stamp 1 -q 0.2 -r 0.8 -p 0.7 --comment "테스트 없음"

# rigs — system rig 보호
opengoose rigs remove evolver    # Error: cannot remove system rig

# skills list — scope + status 표시
opengoose skills list
  Global (installed):
    my-skill           — A test skill                    (installed)
  Rig worker-1 (learned):
    test-before-submit — Use when modifying code and...  (active)
    handle-auth-errors — Use when adding API endpoints.. (dormant)

opengoose skills list --archived   # archived 스킬 표시

# logs — 기존 유지
opengoose logs list
opengoose logs clean --older-than 7d
```

---

## 변경 파일 목록

### 제거
- `crates/opengoose-board/src/board.rs`
- `crates/opengoose-board/src/store.rs`
- `crates/opengoose-board/src/branch.rs`
- `crates/opengoose-board/src/merge.rs`

### 변경 (기존)
- `crates/opengoose-board/src/db_board.rs` → `board.rs` (rename + Board)
- `crates/opengoose-board/src/lib.rs` — re-export 정리
- `crates/opengoose-board/src/entity/stamp.rs` — `evolved_at: Option<DateTime<Utc>>` 추가
- `crates/opengoose-board/src/work_item.rs` — `BoardError::SystemRigProtected` variant 추가
- `crates/opengoose-rig/src/rig.rs` — `Arc<Board>` (no Mutex), Worker async 호출
- `crates/opengoose-rig/src/mcp_tools.rs` — `Arc<Board>` (no Mutex), 전체 async 전환
- `crates/opengoose-rig/src/work_mode.rs` — EvolveMode 추가
- `crates/opengoose-rig/src/middleware.rs` — catalog 로딩 개선, `parse_skill_header` 제거 (load.rs로 통합)
- `crates/opengoose/src/main.rs` — Evolver spawn, --by 제거, system rig
- `crates/opengoose/src/skills/mod.rs` — scope 지원
- `crates/opengoose/src/skills/load.rs` — 3-scope 로딩, catalog 상한, `parse_skill_header` 통합
- `crates/opengoose/src/skills/evolve.rs` — LLM 기반으로 교체
- `crates/opengoose/src/skills/list.rs` — scope + status 표시, `.goose/skills/` → `.opengoose/skills/` 경로 통일

### 신규
- `crates/opengoose/src/evolver.rs` — Evolver 루프 + lazy init (opengoose 크레이트에 배치하여 skills/ 모듈 접근 가능, opengoose-rig에는 EvolveMode/Evolver 타입만)
- `crates/opengoose-board/src/system_rig.rs` — ensure_system_rigs()

### 의존성 방향 주의
`opengoose-rig`는 `opengoose` (바이너리)에 의존할 수 없음. 따라서:
- `EvolveMode` + `Evolver` 타입 정의 → `opengoose-rig`
- Evolver 실행 루프 + 스킬 파일 I/O → `opengoose` (바이너리 크레이트)
- 스킬 로딩/검증 유틸 → `opengoose/src/skills/`

---

## 구현 순서

1. Board 통일 (rename + 제거 + Arc 정리)
2. System rig + CLI --by 제거
3. stamp_notify
4. EvolveMode + Evolver 루프 (lazy init)
5. LLM 기반 스킬 생성 + 검증
6. 스킬 스코프 (디렉토리 구조 + 3-scope 로딩)
7. 스킬 lifecycle (Active/Dormant/Archived)
8. 스킬 효과 추적
9. 테스트 + 검증

---

## 검증 시나리오

```bash
# 1. Board 통일 확인
cargo build && cargo test --workspace

# 2. System rig 확인
opengoose rigs                    # human, evolver 표시
opengoose rigs remove evolver     # Error

# 3. Stamp → 스킬 자동 생성
opengoose board create "Test task"
opengoose board claim 1
opengoose board submit 1
opengoose board stamp 1 -q 0.2 -r 0.8 -p 0.7 --comment "테스트 없음"
# → Evolver가 감지 → LLM 분석 → 스킬 생성

# 4. 스킬 확인
opengoose skills list
# Rig cli (learned):
#   test-before-submit — Use when modifying code and...  (active)

# 5. 스킬 lifecycle
# 30일 후: (dormant) 표시
# 90일 후: archived/로 이동
```
