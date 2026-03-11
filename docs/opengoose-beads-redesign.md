# OpenGoose Beads 재설계: TDD 기반 점진적 구현

> **작성일:** 2026-03-11
> **원칙:** 100% 포팅이 아닌, OpenGoose에 맞는 재설계. 테스트 먼저, 구현 나중.
> **전제:** 단일 바이너리, SQLite 임베디드, Diesel ORM

---

## 1. 세 가지 핵심 기능 상세 설명

### 1.1 Wisp (휘발성 태스크)

#### 무엇인가

Wisp는 **세션 내에서만 존재하는 일회용 태스크**다. 일반 bead가 "영구 메모"라면 Wisp는 "포스트잇"이다.

#### 왜 필요한가

AI 에이전트가 작업 중에 생성하는 모든 것이 영구 기록일 필요는 없다:

```
에이전트가 버그 수정 중:
1. "auth.rs의 line 42가 의심스럽다" ← Wisp (탐색 메모)
2. "login_handler에서 토큰 검증 누락 발견" ← Wisp (중간 발견)
3. "토큰 검증 로직 추가 필요" ← 정식 태스크 (영구)
4. "테스트 통과 확인" ← Wisp (확인 메모)
```

Wisp 없이는 이 모든 것이 work_items에 쌓여 `ready()` 결과를 오염시키고, `prime()` 컨텍스트를 낭비한다.

#### 구현 원리

```
생성 → 사용 → 소멸
  │              │
  ▼              ▼
DB에 저장       두 가지 경로:
(is_ephemeral    a) 자동 삭제 (purge)
 = true)         b) 요약 후 삭제 (squash → 한 줄 기록 → 삭제)
```

**핵심 규칙:**
- `ready()`에 포함되지 않음 (다른 에이전트에게 보이지 않음)
- `prime()`에 포함되지 않음 (컨텍스트 토큰 절약)
- `bd list`에 기본적으로 숨김 (`--include-wisps`로만 조회)
- 의존성(blocks/depends_on) 설정 불가 (독립적 존재)
- 부모-자식 계층 불가 (flat only)
- 세션 종료 시 자동 정리 대상

#### 실제 동작 예시

```rust
// 에이전트가 코드 탐색 중 메모를 남김
let wisp = bead_store.create_wisp("auth.rs:42 — 토큰 만료 처리 누락 의심", "agent-researcher")?;
// → WorkItem { id: 47, is_ephemeral: true, status: Pending, ... }

// 조사 완료 후, 실제 버그를 발견하면 정식 태스크로 전환
let task = bead_store.promote_wisp(wisp.id, "토큰 만료 시 401 대신 500 반환하는 버그")?;
// → wisp 삭제, 새 WorkItem { is_ephemeral: false, ... } 생성

// 또는 중요하지 않으면 그냥 닫음
bead_store.close_wisp(wisp.id)?;
// → 삭제 대기 상태. purge() 호출 시 영구 삭제
```

#### OpenGoose에서의 개선 가능성

Beads의 Wisp는 CLI 기반이라 에이전트가 명시적으로 `bd create --type wisp`를 호출해야 한다.
OpenGoose에서는 **자동 Wisp 감지**가 가능하다:

```rust
// 오케스트레이션 엔진이 에이전트 출력을 분석
// "의심", "확인 필요", "TODO" 등의 패턴 → 자동 Wisp 생성
// 또는: work_item 생명주기가 같은 세션 내에서 완결되면 → 자동 Wisp 전환
```

---

### 1.2 "Landing the Plane" — 세션 종료 프로토콜

#### 무엇인가

AI 에이전트가 세션을 마칠 때 **반드시 수행해야 하는 체크리스트**. 비행기가 활주로에 안전하게 착륙하듯, 에이전트도 세션을 안전하게 착륙시켜야 한다.

#### 왜 필요한가

에이전트 세션이 끝나면 **컨텍스트 윈도우의 모든 내용이 사라진다**. Landing the Plane이 없으면:

```
[위험 시나리오]
에이전트 A가 auth 리팩토링 50% 완료
→ 컨텍스트 윈도우 소진
→ 세션 종료
→ 다음 세션 시작
→ "auth 리팩토링이 어디까지 됐더라?" (기억 없음)
→ 처음부터 코드 분석 다시 시작 (토큰 낭비)
→ 또는 이전 변경과 충돌하는 코드 작성 (회귀 버그)

[안전 시나리오 — Landing the Plane]
에이전트 A가 auth 리팩토링 50% 완료
→ 컨텍스트 윈도우 소진 감지
→ Landing 시작:
  1) 미완료 작업 기록: "session 미들웨어 완료, tower 레이어 전환 WIP"
  2) 테스트 실행: 47 passed
  3) 커밋 & 푸시
→ 다음 세션 시작
→ prime()이 정확한 컨텍스트 제공: "tower 레이어 전환이 다음 작업"
→ 즉시 이어서 작업 시작
```

#### 6단계 프로토콜

```
Step 1: FILE — 미완료 작업 등록
├── 현재 진행 중인 모든 작업의 진행 상황을 태스크로 기록
├── "뇌에만 있는" 정보를 명시적으로 외부화
└── 이유: 다음 세션의 prime()이 이 정보를 제공할 수 있도록

Step 2: GATE — 품질 게이트 실행
├── lint, format, type check, 테스트 실행
├── 실패 시: 고칠 수 있으면 고치고, 못 고치면 P1 태스크로 등록
└── 이유: "깨진 main"을 다음 에이전트에게 넘기지 않기 위해

Step 3: UPDATE — 이슈 상태 갱신
├── 완료된 태스크 닫기 (close with reason)
├── 진행 중인 태스크 상태 메모 추가
├── 차단된 태스크에 차단 사유 기록
└── 이유: ready()가 정확한 결과를 반환하도록

Step 4: SYNC — Git 동기화 (비협상)
├── git add → commit → pull --rebase → push
├── push 실패 시 재시도 (최대 4회, 지수 백오프)
├── 반드시 성공해야 함
└── 이유: 다른 에이전트가 최신 코드를 받아야 하므로

Step 5: VERIFY — 클린 상태 확인
├── git status → clean working tree
├── 프로세스 정리 (개발 서버, 테스트 러너 등)
└── 이유: 다음 세션이 깨끗한 상태에서 시작하도록

Step 6: HANDOFF — 다음 작업 선택 (선택)
├── ready()로 다음 우선순위 태스크 확인
├── 선택적으로 in_progress로 미리 설정
└── 이유: 다음 세션 시작 시간 단축
```

#### 비협상인 이유 (Step 4)

**다중 에이전트 환경에서 push 실패 = 작업 손실:**

```
Agent A: auth 모듈 수정 (push 안 함)
Agent B: auth 모듈 수정 시작 (stale 코드 기반)
Agent B: push 성공
Agent A: 이제 push 시도 → 충돌
→ Agent A의 작업이 conflict resolution 과정에서 손실 위험
```

`git pull --rebase`가 push 전에 반드시 필요한 이유: 다른 에이전트의 변경과 동기화 후 push해야 충돌 최소화.

#### OpenGoose에서의 개선

Beads에서 Landing the Plane은 **에이전트 지침(AGENT_INSTRUCTIONS.md)에 의한 관례적 프로토콜**이다. 프로그래밍적으로 강제되지 않는다.

OpenGoose에서는 **프로그래밍적으로 강제**할 수 있다:

```rust
// opengoose-core/src/engine/mod.rs에 이미 있는 것들:
// - TeamRunCompleted 이벤트
// - OrchestrationRun.complete_run()
// - EventBus.emit()

// 추가할 것:
pub async fn on_agent_session_ending(&self, ctx: &OrchestrationContext) -> Result<LandingReport> {
    let report = LandingReport::new();

    // Step 1: FILE — 미완료 work_items 자동 감지
    let in_progress = ctx.work_items().list_for_run(&ctx.team_run_id, Some(WorkStatus::InProgress))?;
    for item in &in_progress {
        if item.output.is_none() {
            report.warn(format!("WI-{}: 출력 없이 진행 중 — 상태 기록 필요", item.id));
        }
    }

    // Step 2: GATE — 품질 게이트 (설정 가능)
    if let Some(gate_cmd) = config.quality_gate_command() {
        let result = run_command(gate_cmd).await?;
        if !result.success {
            report.fail("품질 게이트 실패", &result.stderr);
        }
    }

    // Step 3: UPDATE — 자동 상태 갱신
    // Wisp 정리
    let purged = ctx.bead_store().purge_ephemeral()?;
    report.info(format!("{purged}개 Wisp 정리 완료"));

    // Step 4: SYNC — EventBus로 알림
    ctx.event_bus().emit(AppEventKind::AgentLanding {
        session_key: ctx.session_key.clone(),
        agent: ctx.current_agent().to_string(),
        report: report.clone(),
    });

    Ok(report)
}
```

**Beads 대비 개선점:**
1. **자동 감지**: 미완료 작업, 깨진 테스트, 더티 상태를 프로그래밍적으로 감지
2. **강제 실행**: 관례가 아닌 코드로 강제 (선택적 opt-out)
3. **보고서 생성**: LandingReport가 다음 세션의 prime()에 포함
4. **이벤트 기반**: EventBus를 통해 모니터링/알림 시스템과 연동

---

### 1.3 Dolt가 유일한 백엔드인 이유

#### 타임라인

```
2024 Q1: Beads 출시 — SQLite 백엔드
2024 Q3: Dolt 백엔드 추가 (대안)
2024 Q4: 다중 에이전트에서 SQLite 문제 발생
2025 Q1: Dolt를 기본 백엔드로 승격
v0.51.0: SQLite 완전 제거 — Dolt만 남음
```

#### SQLite가 실패한 구체적 상황

**문제 1: 쓰기 경합 (Single-Writer Lock)**

```
Agent A: BEGIN; UPDATE work_items SET status='done' WHERE id=1;
         -- 쓰기 락 획득

Agent B: BEGIN; INSERT INTO work_items (title, ...) VALUES ('새 태스크', ...);
         -- SQLITE_BUSY! 락 대기
         -- busy_timeout(5000) 후에도 실패
         -- → 태스크 생성 실패, 에이전트 작업 중단
```

WAL 모드에서도 **쓰기는 단 하나의 연결만** 가능. 에이전트 5개가 동시에 태스크 생성/갱신하면 빈번한 타임아웃 발생.

**문제 2: Last-Writer-Wins (머지 없음)**

```
Base 상태:
| id | title    | status  | priority | assigned_to |
|----|----------|---------|----------|-------------|
| 1  | 버그수정  | open    | 3        | NULL        |

Agent A: UPDATE SET assigned_to='A', status='in_progress' WHERE id=1
Agent B: UPDATE SET priority=1 WHERE id=1  (더 긴급하다고 판단)

SQLite 결과 (Agent B가 나중에 커밋):
| id | title   | status | priority | assigned_to |
|----|---------|--------|----------|-------------|
| 1  | 버그수정 | open   | 1        | NULL        |
                  ↑ A의 상태 변경 사라짐!    ↑ A의 할당 사라짐!

Dolt 결과 (cell-level merge):
| id | title   | status      | priority | assigned_to |
|----|---------|-------------|----------|-------------|
| 1  | 버그수정 | in_progress | 1        | A           |
                  ↑ A의 변경 유지    ↑ B의 변경 유지    ↑ A의 변경 유지
```

**핵심: SQLite는 행 전체를 덮어쓰지만, Dolt는 변경된 셀만 머지한다.**

**문제 3: 히스토리 부재**

```
에이전트가 실수로 중요 데이터 삭제:
  DELETE FROM work_items WHERE priority > 3;  (의도: 낮은 우선순위만 삭제)
  -- 실수로 모든 데이터 삭제됨 (조건 반대)

SQLite: 복구 불가 (백업이 없으면)
Dolt:   SELECT * FROM work_items AS OF 'HEAD~1';  (직전 상태 즉시 조회)
        CALL dolt_reset('--hard', 'HEAD~1');       (롤백)
```

**문제 4: 동기화의 복잡성**

```
SQLite 동기화 (매 세션마다):
  export JSONL → git add → git commit → git push
  → (다른 에이전트) git pull → import JSONL → 충돌 감지? → 수동 해결

Dolt 동기화:
  dolt push → (다른 에이전트) dolt pull → 자동 cell-level merge
```

#### Beads가 Dolt를 선택한 결정적 이유

Steve Yegge의 입장 (블로그/GitHub에서):

> "다중 에이전트 워크플로가 Beads의 핵심 사용 사례다. SQLite의 단일 라이터 제한은 이에 대한 **근본적 차단자**다. 머지 기능만으로도 Dolt의 복잡성은 정당화된다."

결국 **다중 에이전트 동시 쓰기 + 자동 머지**가 핵심. SQLite 위에 이 두 가지를 직접 구축하는 것보다 Dolt를 쓰는 게 더 단순하다는 판단.

#### OpenGoose에 대한 시사점

Beads가 SQLite를 버린 이유를 정면으로 마주해야 한다:

| Beads/Dolt 해결책 | OpenGoose 대안 | 트레이드오프 |
|---|---|---|
| 다중 라이터 (MySQL 프로토콜) | `Mutex<SqliteConnection>` (현재) | 직렬화된 쓰기, 에이전트 5개 이하에서 충분 |
| Cell-level merge | DB-per-Branch + 커스텀 3-way merge | 구현 비용 있으나 핵심 가치 동일 |
| 자동 커밋 | Temporal 테이블 + 트리거 | 모든 변경 자동 기록 |
| `dolt push/pull` | Phase 4: cr-sqlite | 나중에 추가 |
| `AS OF` 쿼리 | `_history` 테이블 조회 | 동일 결과, 다른 구문 |

**결론: OpenGoose는 Dolt 없이도 동일 가치를 달성할 수 있지만, 에이전트 규모가 커지면(20+) 쓰기 경합이 실제 문제가 된다. 그 시점에서 다시 평가.**

---

## 2. OpenGoose 맞춤 재설계: Beads를 넘어서

### 2.1 "100% 포팅하지 않는" 이유

Beads는 **CLI 도구**다. OpenGoose는 **임베디드 오케스트레이션 엔진**이다. 근본적으로 다르다.

| Beads (CLI) | OpenGoose (임베디드) |
|---|---|
| 에이전트가 `bd ready` CLI 호출 | 엔진이 내부적으로 `ready()` 호출 |
| AGENT_INSTRUCTIONS.md로 관례 지시 | 코드로 프로그래밍적 강제 |
| `bd prime` 출력을 시스템 프롬프트에 복사 | `prime()` 출력을 직접 에이전트 컨텍스트에 주입 |
| `bd create`로 수동 태스크 생성 | 오케스트레이션 중 자동 태스크 분해 |
| Dolt 서버 프로세스 필요 | 서버 없음, 단일 바이너리 |
| Git 워크트리에서 동기화 | DB 내부에서 모든 것 처리 |

### 2.2 OpenGoose가 더 잘할 수 있는 것

#### A. 자동 Wisp 감지 (Beads에 없음)

```rust
// Beads: 에이전트가 명시적으로 --type wisp 지정해야 함
// OpenGoose: 오케스트레이션 엔진이 자동 판단

impl BeadStore {
    /// 태스크가 같은 세션 내에서 생성되고 완료되면 → 자동으로 Wisp 처리
    pub fn auto_classify(&self, work_item_id: i32) -> PersistenceResult<()> {
        let item = self.get(work_item_id)?;
        if item.status == WorkStatus::Completed
            && item.created_at == item.session_created_at  // 같은 세션
            && item.output.as_ref().map_or(true, |o| o.len() < 200)  // 짧은 출력
        {
            self.mark_ephemeral(work_item_id)?;
        }
        Ok(())
    }
}
```

#### B. 프로그래밍적 Landing the Plane (Beads에 없음)

Beads는 관례. OpenGoose는 코드로 강제:

```rust
// OrchestrationContext가 세션 종료를 감지하면 자동 실행
// 에이전트가 "Landing the Plane"을 잊어도 엔진이 처리

pub enum LandingCheck {
    Pass(String),
    Warn(String),   // 경고만, 진행 가능
    Fail(String),   // 세션 종료 차단 (선택적)
}

pub struct LandingProtocol {
    checks: Vec<Box<dyn Fn(&OrchestrationContext) -> LandingCheck>>,
}

// 기본 체크:
// 1. 진행 중 태스크에 상태 메모가 있는가?
// 2. 실패한 태스크에 에러가 기록되어 있는가?
// 3. Wisp 정리가 되었는가?
// 4. (선택) 테스트가 통과하는가?
```

#### C. prime()에 이전 세션 LandingReport 포함 (Beads에 없음)

```rust
pub fn prime(&self, session_key: &str) -> String {
    let mut context = String::new();

    // 1. 이전 세션의 Landing Report (Beads에 없음)
    if let Some(report) = self.last_landing_report(session_key)? {
        context += "# Previous Session Summary\n";
        context += &report.brief();  // "3 tasks completed, 2 WIP filed, tests passed"
    }

    // 2. 에이전트 메모리 (Beads의 remember/recall)
    let memories = self.recall(agent_name, None)?;
    if !memories.is_empty() {
        context += "\n# Agent Memories\n";
        for mem in &memories {
            context += &format!("- {}: {}\n", mem.key, mem.value);
        }
    }

    // 3-6: 기존 prime() 로직 (active, ready, recent, blocked, deps)
    // ...

    context
}
```

#### D. Blocked 캐시 + 이벤트 기반 무효화 (Beads보다 효율적)

Beads는 상태 변경 시 캐시를 재계산. OpenGoose는 **EventBus 이벤트로 자동 무효화**:

```rust
// 이미 존재하는 EventBus를 활용
pub fn setup_blocked_cache_invalidation(event_bus: &EventBus, bead_store: Arc<BeadStore>) {
    let mut rx = event_bus.subscribe_reliable();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event.kind {
                AppEventKind::TeamStepCompleted { .. } => {
                    // 태스크 완료 → 이 태스크를 blocker로 가진 캐시 엔트리 무효화
                    bead_store.invalidate_blocked_cache_for_completed(&event);
                }
                _ => {}
            }
        }
    });
}
```

#### E. 적응형 해시 ID + 기존 순차 ID 공존

Beads는 해시 ID만 사용. OpenGoose는 **두 ID 체계를 공존**시킬 수 있다:

```rust
// 기존 코드: work_items.id (INTEGER, 자동 증가) — Diesel FK 등에서 사용
// 신규 추가: work_items.hash_id (TEXT, 해시 기반) — 에이전트 간 참조에 사용

// 내부 로직: 정수 ID 사용 (빠른 조인, FK)
// 외부 노출: 해시 ID 사용 (머지 안전, 에이전트 참조)
// 변환: HashMap<String, i32> 캐시로 O(1) 매핑
```

### 2.3 Beads에서 가져오되 개선할 것

| Beads 기능 | 그대로 가져올 것 | 개선할 것 |
|---|---|---|
| `ready()` | 의존성 해소 로직, 우선순위 정렬 | EventBus 연동, blocked 캐시 이벤트 무효화 |
| `prime()` | BriefIssue 97% 토큰 절감 | + 이전 세션 LandingReport + agent memories |
| `compact()` | 오래된 태스크 요약/아카이브 | OpenGoose event_history와 통합 |
| 해시 ID | 적응형 길이, base36 | 기존 정수 ID와 공존 (내부/외부 이중 키) |
| `remember/recall` | KV 메모리 저장 | prime()에 자동 주입 + 세션별 관련성 필터링 |
| Wisp | 휘발성 태스크 개념 | 자동 분류 + promote-to-task |
| Landing the Plane | 6단계 프로토콜 | 프로그래밍적 강제 + LandingReport |
| 관계 타입 | blocks, depends_on, relates_to | 기존 parent_id와 통합 |

### 2.4 Beads에서 가져오지 않을 것

| Beads 기능 | 이유 |
|---|---|
| Molecule (TOML 워크플로) | OpenGoose에 이미 YAML 기반 Team/Workflow 시스템 있음 |
| Convoy (배달 추적) | 에이전트 워크로드에서 불필요 |
| `bd edit` (에디터 열기) | 임베디드 시스템에서 의미 없음 |
| `bd setup claude/cursor/aider` | OpenGoose는 자체 플랫폼 |
| Dolt 원격 push/pull | Phase 4 (cr-sqlite로 대체) |
| Daemon 모드 (RPC) | OpenGoose가 이미 서버 역할 |
| Git 훅 자동 설정 | 임베디드이므로 불필요 |
| `bd doctor` (상태 복구) | 대신 Diesel 마이그레이션 + SQLite PRAGMA integrity_check |
| `bd query` (SQL 직접 실행) | Diesel ORM이 이 역할 수행 |

---

## 3. TDD 기반 구현 계획

### 3.1 테스트 먼저 작성 순서

```
Phase 0: 테스트 인프라 (기존 패턴 활용)
├── test helper: bead_test_db() → Arc<Database> + BeadStore
├── test helper: create_test_bead(store, title) → WorkItem
└── test helper: create_test_relationship(store, from, to, type)

Phase 1: 핵심 데이터 모델 (테스트 먼저)
├── 1a. 해시 ID
│   ├── TEST: hash_id_has_bd_prefix
│   ├── TEST: hash_id_uniqueness_same_title_different_time
│   ├── TEST: hash_id_adaptive_length_under_500
│   ├── TEST: hash_id_adaptive_length_over_500
│   ├── TEST: hash_id_collision_retry_with_nonce
│   └── IMPL: hash_id.rs (~60줄)
│
├── 1b. 관계 + 순환 감지
│   ├── TEST: add_blocks_relationship
│   ├── TEST: add_depends_on_relationship
│   ├── TEST: detect_direct_cycle (A→B→A)
│   ├── TEST: detect_transitive_cycle (A→B→C→A)
│   ├── TEST: allow_non_cyclic_graph
│   ├── TEST: remove_relationship
│   └── IMPL: relationships.rs (~100줄)
│
├── 1c. Wisp
│   ├── TEST: create_wisp_sets_ephemeral
│   ├── TEST: wisp_excluded_from_ready
│   ├── TEST: wisp_excluded_from_prime
│   ├── TEST: promote_wisp_to_task
│   ├── TEST: purge_clears_closed_wisps
│   ├── TEST: purge_keeps_open_wisps
│   └── IMPL: work_items.rs 확장 (~40줄)

Phase 2: 핵심 알고리즘
├── 2a. ready()
│   ├── TEST: ready_returns_pending_only
│   ├── TEST: ready_excludes_blocked_by_open_blocker
│   ├── TEST: ready_includes_after_blocker_completes
│   ├── TEST: ready_excludes_unmet_dependencies
│   ├── TEST: ready_respects_priority_order
│   ├── TEST: ready_limits_batch_size
│   ├── TEST: ready_filters_by_session
│   ├── TEST: ready_includes_unassigned
│   ├── TEST: ready_excludes_wisps
│   └── IMPL: ready.rs (~80줄)
│
├── 2b. prime()
│   ├── TEST: prime_includes_active_tasks
│   ├── TEST: prime_includes_ready_tasks
│   ├── TEST: prime_includes_recent_completions
│   ├── TEST: prime_includes_blocked_with_reason
│   ├── TEST: prime_includes_agent_memories
│   ├── TEST: prime_includes_last_landing_report
│   ├── TEST: prime_uses_brief_format (token count < 2000)
│   ├── TEST: prime_excludes_wisps
│   └── IMPL: prime.rs (~120줄)
│
├── 2c. compact()
│   ├── TEST: compact_groups_by_parent
│   ├── TEST: compact_preserves_key_outputs
│   ├── TEST: compact_marks_originals_as_compacted
│   ├── TEST: compact_ignores_recent_completions
│   ├── TEST: compacted_excluded_from_ready
│   ├── TEST: compacted_excluded_from_prime
│   └── IMPL: compact.rs (~80줄)

Phase 3: 에이전트 지원
├── 3a. remember/recall
│   ├── TEST: remember_stores_kv
│   ├── TEST: remember_upserts_on_same_key
│   ├── TEST: recall_returns_all_for_agent
│   ├── TEST: recall_filters_by_keyword
│   ├── TEST: forget_removes_entry
│   ├── TEST: memories_isolated_per_agent
│   └── IMPL: memory.rs (~60줄)
│
├── 3b. Landing the Plane
│   ├── TEST: landing_detects_in_progress_without_output
│   ├── TEST: landing_purges_wisps
│   ├── TEST: landing_generates_report
│   ├── TEST: landing_report_included_in_next_prime
│   └── IMPL: session_hooks.rs (~100줄)
│
├── 3c. Blocked 캐시
│   ├── TEST: cache_populated_on_relationship_add
│   ├── TEST: cache_invalidated_on_blocker_complete
│   ├── TEST: cache_handles_transitive_blocks
│   ├── TEST: ready_uses_cache_when_available
│   └── IMPL: ready.rs 확장 (~60줄)
```

### 3.2 의존성 최소화

```toml
# 추가할 크레이트:
petgraph = "0.8"    # DAG (관계 그래프, 순환 감지)
sha2 = "0.10"       # SHA-256 (해시 ID)
# base36: 직접 구현 (~15줄)

# 이미 있는 것 (추가 불필요):
# diesel (sqlite) — ORM
# serde_json — JSON 직렬화 (tags 등)
# chrono — 시간 처리
# uuid — 대안으로 사용 가능하나 해시 ID가 더 적합
```

### 3.3 기존 코드 활용 맵

```
기존 OpenGoose          →   Beads 기능에 활용
─────────────────────────────────────────────────
WorkItem + WorkStatus   →   Bead 데이터 모델 기반
WorkItemStore           →   BeadStore가 래핑
parent_id               →   subtask_of 관계
find_resume_point()     →   세션 복구 로직
EventBus                →   blocked 캐시 무효화, Landing 알림
AppEventKind            →   새 이벤트 추가 (AgentLanding 등)
EventStore              →   audit trail, 시간여행
SessionStore            →   세션 키 관리
MessageQueue            →   에이전트 간 조정
OrchestrationContext    →   BeadStore 주입 포인트
Database::open_in_memory →  테스트 인프라
db_enum! 매크로         →   새 enum 정의 (RelationType, Priority 등)
```

---

## 4. Phase 간 의존성

```
Phase 0: 테스트 인프라         (선행 조건 없음)
    │
    ▼
Phase 1: 데이터 모델          (마이그레이션 + 모델)
    │
    ├──▶ Phase 2: 알고리즘     (ready/prime/compact)
    │       │
    │       ├──▶ Phase 3: 에이전트 지원  (memory, landing, cache)
    │       │
    │       └──▶ Phase 4(별도): VCS 브랜칭  (dolt-beads-porting-guide.md 참조)
    │
    └──▶ Phase 5(나중): 연합 동기화 (cr-sqlite)
```

**Phase 1-3이 Beads 포팅. Phase 4가 Dolt 포팅. 독립적으로 진행 가능.**

---

## 5. 성공 지표

| 지표 | 목표 | 측정 방법 |
|------|------|----------|
| `ready()` 정확성 | 차단된 태스크 0건 반환 | 테스트 케이스 |
| `prime()` 토큰 효율 | < 2,000 토큰 (100개 태스크 기준) | 바이트 수 / 4 |
| `compact()` 압축률 | 완료 태스크 90%+ 압축 | 테스트 케이스 |
| Wisp 정리 | 세션 종료 시 100% 정리 | Landing 테스트 |
| Landing 완료율 | 모든 세션이 Landing 수행 | 이벤트 로그 |
| 테스트 커버리지 | Phase 1-3 코드 80%+ | cargo tarpaulin |
| 빌드 시간 증가 | < 5초 | CI 측정 |
| 바이너리 크기 증가 | < 1MB | 빌드 비교 |
