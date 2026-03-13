# OpenGoose v2 아키텍처 설계서

> **작성일:** 2026-03-11
> **최종 수정:** 2026-03-13
> **목표:** Gas Town/Wasteland 수준의 멀티에이전트 오케스트레이션을 Goose-native로 달성
> **원칙:** Goose가 제공하는 것은 100% 재사용, 없는 것만 최소한으로 구축
> **스토리지:** SQLite + Diesel (현재) → prollytree 전면 전환 (개발 단계, 공격적 진행)

---

## 1. 설계 철학

### 1.1 Gas Town의 교훈

Steve Yegge는 Gas Town을 17일간 "100% vibecoding"으로 구축했다 — 300+ Go 파일, 75k LOC. 이 과정에서 밝혀진 핵심 통찰:

- **"설계가 병목"** (Maggie Appleton) — 에이전트 수가 늘어날수록 작업 분배와 충돌 관리가 코딩보다 어렵다
- **"Propulsion principle" (GUPP)** — Hook에 있으면 실행한다. Pull 기반 실행 모델
- **"컨텍스트는 유한하다"** — 오케스트레이션과 실행을 반드시 분리해야 한다
- **"Write-as-you-go"** — 중간 결과를 디스크에 지속적으로 저장한다 (에이전트가 죽어도 작업 보존)
- **"Delegation >> Doing"** — 오케스트레이터는 실행하지 않고 위임만 한다

### 1.2 Goose-native vs 독자 구현: 경계선

```
┌─────────────────────────────────────────────────────────────┐
│                   Goose가 제공 (재사용)                       │
│                                                             │
│  Agent::reply()    SessionManager    Recipe/sub_recipes     │
│  Provider          ExtensionManager  PermissionManager      │
│  MCP dispatch      GooseMode         CancellationToken      │
│  fix_conversation  context_mgmt      SubagentRunParams      │
└─────────────────────────────────────────────────────────────┘
                         ↕
┌─────────────────────────────────────────────────────────────┐
│              OpenGoose가 구축 (최소한으로)                    │
│                                                             │
│  멀티채널 어댑터     팀 오케스트레이션    Witness 감독         │
│  (Discord/Slack/    (Chain/FanOut/      (stuck/zombie       │
│   Telegram/Matrix)   Router)            감지)               │
│                                                             │
│  Agent Map 뷰       MCP 팀 도구         Git worktree        │
│  (실시간 시각화)     (에이전트 간 통신)    격리               │
│                                                             │
│  Beads 알고리즘     prollytree 스토리지                      │
│  (ready/prime/      (구조적 공유,                            │
│   compact)          3-way merge)                            │
└─────────────────────────────────────────────────────────────┘
```

### 1.3 Gas Town과의 근본적 차이

| | Gas Town | OpenGoose v2 |
|--|----------|-------------|
| **기반** | 모든 것을 Go로 처음부터 구현 (75k LOC) | Goose 라이브러리 위 오케스트레이션 레이어 |
| **에이전트** | `goose` CLI를 Tmux에서 직접 실행 | Goose `Agent::reply()` API를 Rust에서 호출 |
| **DB** | Dolt SQL (별도 서버) | **prollytree** (순수 Rust, 단일 바이너리, 3-way merge) |
| **격리** | Git worktree + Tmux + Dolt branch | Goose Session + 선택적 Git worktree + prollytree branch |
| **통신** | 자체 Mail 시스템 (Go) | MCP Stdio 서버 (기존 MessageBus 재사용) |
| **이점** | 완전한 제어 | Goose 업스트림 개선사항 자동 흡수, 멀티채널 지원 |

이것은 약점이 아니라 **전략적 선택**이다. Goose가 서브에이전트, 퍼미션, 컨텍스트 관리를 개선할 때마다 OpenGoose는 자동으로 혜택을 받는다.

---

## 2. Crate 구조 가이드라인

### 2.1 레이어 정의

```
Layer 0 - opengoose-types (공유 도메인)
    ├── 타입: WorkItem, AppEventKind, WorkStatus, RelationType
    ├── 트레잇: BeadsRead, BeadsMaintenance, BeadsPrimeSource
    └── 의존성: serde, chrono, uuid (Diesel/Goose/Tokio 없음)
              ↓
Layer 1 - opengoose-persistence (SQLite/prollytree 어댑터)
    ├── 트레잇 구현: ready(), compact(), prime_snapshot()
    ├── 모듈: work_items, relationships, ready, compact, prime_data, hash_id, wisps
    └── Diesel/SQLite/prollytree 전용, 정책 로직 없음
              ↓
Layer 2 - opengoose-core (최소 코어)
    ├── DB 불가지론적 (트레잇 객체로 접근)
    └── prime 포맷팅: format_prime(snapshot, token_budget) → String
              ↓
Layer 3 - opengoose-teams (오케스트레이션)
    ├── Witness: witness.rs (EventBus + 타이머)
    ├── 실행 전략: Chain, FanOut, Router
    └── 팀 정책: 언제 ready() 호출, 언제 compact() 실행
              ↓
Layer 4 - 인터페이스
    ├── opengoose-web: Agent Map (SSE + Askama)
    ├── opengoose-team-tools: MCP Stdio 서버 (독립 바이너리)
    └── opengoose-cli, discord, slack, telegram
```

### 2.2 핵심 규칙

1. **하위 레이어는 상위 레이어에 의존 금지**
2. **Diesel/SQLite/prollytree는 `opengoose-persistence`에서만 사용**
3. **프롬프트/템플릿 로직은 `core`/`teams`에 배치**
4. **MCP team-tools는 독립 바이너리, core/teams 의존 금지**

### 2.3 트레잇 기반 분리

```rust
// opengoose-types에 정의
pub trait BeadsRead {
    fn ready(&self, opts: &ReadyOptions) -> anyhow::Result<Vec<WorkItem>>;
}

pub trait BeadsPrimeSource {
    fn prime_snapshot(&self, team_run_id: &str, agent_name: &str) -> anyhow::Result<PrimeSnapshot>;
}

pub trait BeadsMaintenance {
    fn compact(&self, team_run_id: &str, older_than: chrono::DateTime<Utc>) -> anyhow::Result<()>;
}

// opengoose-persistence에서 구현
impl BeadsRead for SqliteStore { ... }
impl BeadsPrimeSource for SqliteStore { ... }

// opengoose-core/teams에서 트레잇 객체로 사용
fn execute_team(store: &dyn BeadsRead, ...) { ... }
```

---

## 3. 에이전트 격리 및 자율성

### 3.1 Gas Town의 격리 모델

Gas Town은 세 겹의 격리를 제공한다:

```
┌─ Tmux Session (프로세스 격리) ─┐
│  ┌─ Git Worktree (코드 격리) ─┐ │
│  │  ┌─ Branch (데이터) ─────┐│ │
│  │  │  Polecat Agent       ││ │
│  │  └──────────────────────┘│ │
│  └──────────────────────────┘ │
└───────────────────────────────┘
```

- **Tmux session**: 에이전트별 독립 프로세스, beacon으로 생존 확인
- **Git worktree**: `git worktree add`로 에이전트별 코드 카피 생성
- **Branch**: 에이전트별 브랜치로 데이터 충돌 없이 병렬 수정

### 3.2 OpenGoose의 격리 전략

Goose-native 격리를 우선하되, 필요에 따라 Gas Town 수준으로 확장:

**Layer 1: 세션 격리 (Goose-native, 즉시 가능)**
```rust
// Goose SessionType으로 에이전트별 독립 세션
let session = session_manager.create_session(
    working_dir.clone(),
    format!("team-{}-agent-{}", team_name, agent_name),
    SessionType::SubAgent,
).await?;
```

- 각 에이전트가 독립 Conversation (대화 이력)
- 독립 Extension 상태 (도구 설정/캐시)
- 독립 토큰 회계

**Layer 2: 퍼미션 격리 (Goose-native, 즉시 가능)**
```yaml
# 역할별 GooseMode 차등 적용
researcher:
  goose_mode: chat_only  # 도구 사용 불가, 정보 수집만
developer:
  goose_mode: smart_approval  # 위험한 도구만 확인
reviewer:
  goose_mode: manual_approval  # 모든 도구 사용 전 확인
```

**Layer 3: 작업 디렉토리 격리 (Git worktree, Phase 2)**
```bash
# 에이전트별 독립 코드 카피
git worktree add /tmp/opengoose-agent-{name} -b agent/{name}/{run_id}
```

- 에이전트가 독립 브랜치에서 코드 수정
- 완료 시 main에 머지 시도 → 충돌 시 "re-imagine"
- worktree 자동 정리 (완료/실패 후 삭제)

### 3.3 Polecat 상태머신 — Goose-native 구현

Gas Town의 Polecat 상태:
```
Working → Idle → Stuck → Zombie → Done
```

OpenGoose에서 Goose AgentEvent 스트림 위에 구현:

```
    +---------+
    |  Idle   |<──────────────────────+
    +----+----+                       |
         | TeamStepStarted (AppEvent) |
    +----v----+                       |
    | Working |───────────────────────+
    +----+----+  TeamStepCompleted
         |
    no AgentEvent for stuck_timeout (기본 5분)
    +----v----+
    |  Stuck  |──→ AgentStuck 이벤트 emit, 대시보드 경고
    +----+----+
         |
    no AgentEvent for zombie_timeout (기본 10분)
    +----v----+
    | Zombie  |──→ CancellationToken::cancel() + retry or fail
    +---------+
```

**핵심 원리**: AgentEvent 스트림의 **아무 이벤트라도** 수신되면 에이전트가 살아있다고 판단. Message뿐 아니라 McpNotification(도구 사용), ModelChange, HistoryReplaced(컨텍스트 압축) 모두 liveness 증거로 활용.

**GUPP 감지 (Gastown 패턴)**: Hook에 작업이 있는데 실행하지 않는 에이전트 탐지 → Witness가 경고 또는 자동 재할당.

**구현 위치**: `crates/opengoose-teams/src/witness.rs` (**✅ 완성, 304줄, 14개 테스트**)
- `spawn_witness(event_bus, config)` → tokio 백그라운드 태스크
- `EventBus::subscribe_reliable()` 사용 → 이벤트 누락 없음
- 5초 간격 타이머로 타임아웃 체크
- `WitnessHandle`의 `agents: Arc<DashMap<String, AgentStatus>>` → Agent Map에서 직접 조회

**타임아웃 설계 근거:**
- `stuck_timeout = 300초 (5분)`: Goose Agent의 일반적인 MCP 도구 호출은 30초-2분 내 완료. 5분 무응답은 LLM 루프 또는 네트워크 hang 가능성 높음. Gas Town의 beacon 간격(60초)에 여유 배수(5x) 적용.
- `zombie_timeout = 600초 (10분)`: stuck 상태에서 추가 5분 — 긴 빌드/테스트(cargo build 등)도 10분 이상이면 비정상. CancellationToken::cancel() 후 작업 재할당.
- **liveness 이벤트 정의**: `TeamStepStarted`, `TeamStepCompleted`, `TeamStepFailed`는 명시적 타이머 리셋. `ModelChanged`, `ContextCompacted`, `ExtensionNotification`은 암묵적 liveness 신호 (Working 상태의 에이전트만).
- **false positive 대응**: MCP 도구가 3분+ 실행 중이면 `ExtensionNotification` 이벤트가 발생하므로 타이머 리셋됨. 도구 호출 없이 LLM만 응답 대기하는 경우에만 stuck 판정.
- **zombie 후 처리**: `AgentZombie` 이벤트 → EventBus broadcast → TeamOrchestrator가 `CancellationToken::cancel()` 호출 + 작업을 다른 에이전트에게 재할당 또는 실패 처리

### 3.4 Witness vs Deacon — Goose-native 대응

| Gas Town 역할 | 기능 | OpenGoose 대응 |
|---|---|---|
| **Mayor** | 인간 대리인, 코드 미작성 | 사용자 (채널 메시지 전송자) |
| **Witness** | 에이전트 헬스 순찰, stuck 감지 | `witness.rs` — EventBus 구독 + 타이머 |
| **Deacon** | 백그라운드 작업 실행 | Goose Recipe + `opengoose schedule` 명령 |
| **Polecat** | 일회성 작업자 | FanOut/Chain executor의 에이전트 |
| **Dogs** | 유지보수 (압축, 아카이빙) | Goose `context_mgmt` 자동 압축 |
| **Refinery** | 머지 큐 | Phase 2 Git worktree + 머지 전략 |

---

## 4. 에이전트 간 통신

### 4.1 현재의 문제: 텍스트 파싱

```
에이전트 → LLM 출력: "@reviewer: please check this\n[BROADCAST]: found a bug"
           ↓
parse_agent_output() → { delegations: [("reviewer", "...")], broadcasts: ["found a bug"] }
```

**문제점:**
- LLM이 포맷을 지키지 않으면 파싱 실패 (비결정성)
- Goose의 도구 검사 파이프라인 (보안, 퍼미션, 반복) 우회
- 디버깅이 어려움 (텍스트 파싱 vs 구조화된 도구 호출)

**기존 인프라 (이미 구현됨, 그러나 에이전트가 직접 접근 불가):**
- `MessageQueue` (SQLite 기반): `pending→processing→completed/dead`, 재시도 로직 내장
- `AgentMessageStore`: `send_directed(from, to, payload)`, `publish(from, channel, payload)` — 방향성/상태 추적
- `MessageBus` (Tokio broadcast): 실시간 인메모리 이벤트 전달

**Goose subagent 시스템과의 차이:**
Goose의 subagent는 **순차적 부모-자식** 관계에 최적화 (Summon 기반, 한 번에 하나). OpenGoose 팀은 **비계층적 형제 간 통신** + **비동기 위임** + **브로드캐스트**가 필요 — subagent만으로는 불충분.

### 4.2 참조 모델 비교

| 시스템 | 통신 방식 | 특징 |
|---|---|---|
| **Gas Town Mail** | 구조화된 메일 큐 (Go) | 4 priority × 4 type, JSONL 이벤트 로깅 |
| **Goosetown gtwall** | Bash 파일 기반 append-only | 파일 잠금, ~400줄, 단순/효과적 |
| **TinyClaw** (별개 프로젝트) | SQLite WAL 큐 | pending→processing→completed/dead |
| **OpenGoose 현재** | 텍스트 파싱 | 불안정, 하지만 MessageBus/AgentMessageStore 인프라는 존재 |

### 4.3 권장: MCP Stdio 서버 기반 팀 도구

**새 크레이트: `opengoose-team-tools`** — Rust로 구현한 MCP Stdio 서버

```
team__delegate(agent, message)     → AgentMessageStore.send_directed()
team__broadcast(message)           → AgentMessageStore.publish("broadcast")
team__read_broadcasts(since_id?)   → AgentMessageStore.channel_history("broadcast")
team__send_message(to, message)    → AgentMessageStore.send_directed()
team__read_messages()              → AgentMessageStore.receive_pending()
```

에이전트 실행 시 자동 등록:
```rust
// runner.rs에서 팀 실행 시 자동으로 team-tools extension 추가
let config = ExtensionConfig::Stdio {
    name: "team-tools".into(),
    cmd: "opengoose-team-tools".into(),
    envs: HashMap::from([
        ("OPENGOOSE_TEAM_RUN_ID", team_run_id),
        ("OPENGOOSE_AGENT_NAME", agent_name),
        ("OPENGOOSE_DB_PATH", db_path),
        ("OPENGOOSE_TEAM_MEMBERS", members.join(",")),
    ]),
    ..
};
```

**gtwall 패턴 참고:**
- 파일 기반 append-only 브로드캐스트의 단순함
- 필수 cadence: 시작 시 알림, 3-5 tool call마다 읽기, 발견 즉시 공유

**MCP가 필요한 근본적 이유:**

| 요구사항 | Goose subagent | 텍스트 파싱 | **MCP 팀 도구** |
|---|---|---|---|
| 비계층적 형제 통신 | ❌ 부모-자식만 | ✅ 텍스트로 가능 | ✅ 구조화된 JSON |
| Goose 보안 파이프라인 | ✅ 자동 적용 | ❌ 우회 | ✅ 자동 적용 |
| 비동기 위임 (완료 대기 없이) | ❌ Summon은 블로킹 | ✅ | ✅ |
| 브로드캐스트 (1:N) | ❌ | ✅ 파싱 불안정 | ✅ |
| 상태 추적 (delivered/ack) | ❌ | ❌ | ✅ AgentMessageStore |
| 디버깅/로깅 | ✅ | ❌ | ✅ MCP 도구 호출 기록 |

**이점:**
- Goose의 도구 검사 파이프라인 자동 적용 (보안, 퍼미션, 반복 체크)
- 구조화된 JSON (텍스트 파싱 제거)
- 기존 MessageBus + AgentMessageStore 100% 재사용
- `CommunicationMode::McpTools` 플래그로 점진적 마이그레이션

**구현 상태:** `opengoose-team-tools` 크레이트 존재 (MCP JSON-RPC + 5개 도구 정의 + DB 연결). 도구 호출 권한 검사 연동 필요.

---

## 5. 머지 및 충돌 해결

### 5.1 Gas Town Refinery의 "Re-imagine" 패턴

Gas Town의 핵심 통찰:

> LLM이 생성한 코드는 "다시 생성"이 저렴하다. ours/theirs 충돌 해결 대신, 두 결과를 컨텍스트로 주고 새로운 통합 구현을 요청하면 된다.

**Refinery 흐름:**
```
Agent A의 코드 ─┐
               ├──→ LLM "re-imagine" ──→ 통합된 새 코드
Agent B의 코드 ─┘
```

### 5.2 OpenGoose 적용

Phase 2에서 Git worktree 격리와 함께 구현:

1. 에이전트별 독립 브랜치에서 작업
2. 완료 시 main에 머지 시도
3. 충돌 발생 시:
   - 충돌 파일과 양쪽 변경 내용 추출
   - LLM에게 "re-imagine" 요청
   - 새 통합 코드로 커밋

**prollytree 활용:**
- 구조적 공유로 브랜치 생성 비용 최소화
- 3-way merge로 충돌 감지
- ConflictResolver 커스텀 구현 가능

---

## 6. 데이터 인프라

### 6.1 스토리지 전략

**현재:** SQLite + Diesel (13 테이블, Beads 4기능 구현 완료)

**목표:** prollytree 전면 전환 — 개발 단계이므로 공격적으로 진행
- `opengoose-prolly` 크레이트 준비 완료 (749줄, 24개 테스트)
- prollytree v0.3.2-beta (GitHub main, `git` feature only)
- `ProllyStore`: CRUD + 관계 관리 + Merkle 증명 + diff
- `VersionedWorkItemStore`: Git 백엔드 (branch/commit/merge)
- `WorkItemStatusResolver`: 충돌 해결 (completed > failed > in_progress > ...)
- SQLite → prollytree 직접 전환 (호환성 보장 불필요, Dual-Write 단계 생략 가능)

**No Dolt:** 외부 서버 의존성 제거

### 6.2 WorkItem → Beads 수준으로

Beads 기능을 WorkItem 테이블 확장으로 대응:

| Beads | 현재 WorkItem | 확장 방안 |
|---|---|---|
| 해시 기반 ID (`bd-a1b2`) | 자동 증가 정수 | SHA256 + base36, 적응형 길이 |
| 중첩 계층 (`bd-a3f8.1.1`) | `parent_id` 1레벨 | Materialized path 컬럼 |
| 관계 (`blocks`, `depends_on`) | 없음 | `work_item_relations` 테이블 + petgraph |
| `bd prime` (컨텍스트) | 없음 | prime_snapshot() + format_prime() |
| `bd ready` (미차단 작업만) | `status` 필터 | 3-step 알고리즘 + 캐싱 |
| `bd compact` (메모리 축소) | 없음 | 2-tier compaction (30일/90일) |
| Wisp (임시 작업) | 없음 | `is_ephemeral` + squash/burn |
| waits-for 게이트 | 없음 | FanOut 완료 대기 (all/any children) |

### 6.3 prollytree 전환 시점

**전환 전략:** 개발 단계이므로 호환성 부담 없이 직접 전환
1. opengoose-prolly의 `ProllyStore`/`VersionedWorkItemStore`를 프로덕션 스토리지로 승격
2. Diesel/SQLite 의존성 제거 (13개 테이블 → ProllyStore KV 매핑)
3. 기존 Beads 알고리즘(ready/prime/compact)을 prollytree 위에서 재구현
4. Git-backed VCS를 기본 활성화 (branch-per-agent 즉시 사용)

**벤치마크 기준 (전환 중 측정):**
- INSERT 1000개 work_items: 목표 < 1초
- 브랜치 생성 + 100개 변경 + merge: 목표 < 500ms
- diff(branch_a, branch_b): 목표 O(변경), < 100ms for 1000 items
- 메모리: 10개 브랜치 × 1000 items, 구조적 공유로 < 50MB

**전환 경로**: `docs/20-architecture/storage.md` 참조

---

## 7. 연합/분산 (Wasteland 대응)

### 7.1 Wasteland의 핵심 개념

| 개념 | 설명 |
|---|---|
| **Wanted Board** | 작업 게시판 (open → claimed → in_review → completed) |
| **Stamps** | 다차원 평판 (Quality, Reliability, Creativity) + severity 가중치 |
| **Trust Ladder** | outsider → newcomer → contributor → trusted → maintainer |
| **Yearbook Rule** | 자기 작업은 자기가 검증 못 함 (`author ≠ subject`) |
| **Federation** | 분산 인스턴스 간 작업 조정, HOP URI 기반 이식 가능한 ID |

### 7.2 OpenGoose의 단계별 접근

**Phase 1: 단일 인스턴스 내 평판 (단기)**

기존 WorkItem 완료 이력을 기반으로:
```sql
-- 에이전트별 성공률 계산
SELECT assigned_agent,
       COUNT(CASE WHEN status = 'completed' THEN 1 END) * 100.0 / COUNT(*) as success_rate,
       AVG(julianday(updated_at) - julianday(created_at)) as avg_duration_days
FROM work_items
WHERE assigned_agent IS NOT NULL
GROUP BY assigned_agent;
```

- 성공률 높은 에이전트에게 중요한 작업 우선 할당
- Yearbook Rule: `reviewer` 역할은 자신이 `developer`로 참여한 작업을 검토 불가
  - `work_items.assigned_agent != review_request.reviewer` 제약

**Phase 4: Stamps 다차원 평판 (장기)**

새 테이블: `agent_stamps`
```sql
CREATE TABLE agent_stamps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    agent_name TEXT NOT NULL,
    work_item_id INTEGER NOT NULL,
    dimension TEXT NOT NULL,  -- quality, reliability, creativity
    score REAL NOT NULL,      -- -1.0 to 1.0
    severity TEXT NOT NULL,   -- leaf (1), branch (3), root (5)
    stamped_by TEXT NOT NULL,  -- Yearbook Rule: stamped_by != agent_name
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (work_item_id) REFERENCES work_items(id),
    CHECK (stamped_by != agent_name)
);
```

**Phase 4: 멀티 인스턴스 연합 (장기)**

기존 `RemoteAgent` WebSocket 프로토콜을 인스턴스 간으로 확장:

```
OpenGoose Instance A                    OpenGoose Instance B
┌───────────────────┐                  ┌───────────────────┐
│  Agent: researcher│  ←──WebSocket──→ │  Agent: developer │
│  Agent: reviewer  │  ProtocolMessage │  Agent: writer    │
│  RemoteAgentRegistry                 │  RemoteAgentRegistry
└───────────────────┘                  └───────────────────┘
         ↕ prollytree sync                     ↕
    ┌────────────┐                        ┌────────────┐
    │  Storage   │  ←── 구조적 공유 ──→   │  Storage   │
    └────────────┘                        └────────────┘
```

- `ProtocolMessage::Handshake`의 `capabilities`로 에이전트 능력 광고
- `ProtocolMessage::MessageRelay`로 크로스 인스턴스 작업 위임
- prollytree 3-way merge로 데이터 동기화
- Trust Ladder: API key 인증 + 작업 이력 기반 신뢰 수준

---

## 8. 웹 대시보드 진화: Agent Map

### 8.1 현재 대시보드의 한계

`opengoose-web`은 이미 풍부한 대시보드를 제공 (Dashboard, Sessions, Runs, Agents, Teams, Queue, API Keys — 7개 페이지):
- 집계 통계 중심 (활성 세션 수, 큐 깊이 등)
- 개별 에이전트의 **실시간 상태**가 없음
- 팀 실행 중 에이전트가 무엇을 하고 있는지 볼 수 없음

### 8.2 설계 참조: TinyClaw TinyOffice + Goosetown Village Map

두 개의 **별개 프로젝트**에서 각각의 강점을 차용한다.

**TinyClaw — TinyOffice (Office View)** `docs/10-references/tinyclaw/README.md`
- **핵심 가치: 실용적 정보 밀도** — 한 화면에서 모든 에이전트의 상태를 즉시 파악
- 에이전트별 카드: 이름, 현재 작업 제목, 상태(active/idle/stuck), 경과 시간
- SQLite WAL 큐 기반 작업 상태 추적 (pending→processing→completed/dead)
- **차용할 것**: 정보 밀도 — 에이전트 이름, 작업 제목, 상태, 경과 시간을 카드 한 장에 압축

**Goosetown — Village Map** `docs/10-references/goosetown/README.md`
- **핵심 가치: 시각적 생동감** — 에이전트가 "살아있다"는 느낌
- 역할별 건물 배치 (Hall=오케스트레이터, Library=리서처, Factory=워커, Scriptorium=라이터)
- A* 경로탐색 기반 에이전트 이동 애니메이션 (~700줄 village.js, 160px/sec)
- SSE 기반 실시간 업데이트 + gtwall 메시지 → 8초 표시 말풍선
- Lit-html 컴포넌트 (빌드 단계 없음), 옵저버 패턴 상태 관리
- **차용할 것**: 실시간 SSE 업데이트, 메시지 말풍선, 상태 변화 애니메이션

**OpenGoose Agent Map = TinyOffice의 정보 밀도 + Village Map의 시각적 생동감**

### 8.3 Agent Map 설계

```
┌─────────────────────────────── Agent Map ──────────────────────────────┐
│                                                                        │
│  ┌── Metrics ────────────────────────────────────────────────────────┐  │
│  │  Active runs: 2   Tracked agents: 5   Success: 87%   Witness: ✅  │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│  ┌── Agent Cards (TinyOffice 스타일 정보 밀도) ─────────────────────┐  │
│  │                                                                   │  │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐  │  │
│  │  │ 🟢 researcher    │  │ 🟢 developer    │  │ 🟡 reviewer     │  │  │
│  │  │ team: feature-dev │  │ team: feature-dev│  │ team: feature-dev│  │  │
│  │  │                  │  │                  │  │                  │  │  │
│  │  │ Working          │  │ Working          │  │ Idle             │  │  │
│  │  │ 3m 12s           │  │ 1m 45s           │  │ —                │  │  │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│  ┌── Message Flow (Village Map 스타일 말풍선) ───────────────────────┐ │
│  │  💬 researcher → developer : "API 스펙 발견, endpoint 3개"       │ │
│  │  💬 developer  → reviewer  : "구현 완료, 리뷰 요청"              │ │
│  │  📢 [BROADCAST] researcher : "rate limit 주의"                   │ │
│  └───────────────────────────────────────────────────────────────────┘ │
│                                                                        │
│  ┌── Team Topology (실행 전략 시각화) ──────────────────────────────┐ │
│  │  [researcher] ──→ [developer] ──→ [reviewer]     (Chain)         │ │
│  │  [researcher-1] ──┐                                               │ │
│  │  [researcher-2] ──┼──→ [synthesizer]              (FanOut)        │ │
│  │  [researcher-3] ──┘                                               │ │
│  └───────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────┘
```

### 8.4 데이터 아키텍처

**이미 구현된 데이터 소스:**

| 데이터 | 소스 | 상태 | 연결 방법 |
|---|---|---|---|
| 에이전트 상태 (state, elapsed) | `WitnessHandle.agents: Arc<DashMap<String, AgentStatus>>` | ✅ 구현됨 | 직접 조회 |
| 에이전트 상태 열거 | `AgentState { Idle, Working, Stuck, Zombie }` | ✅ 구현됨 | — |
| 현재 작업 제목 | `WorkItemStore` → `assigned_to` + `status=InProgress` | ✅ 구현됨 | SQL 쿼리 |
| 메시지 플로우 | `AgentMessageStore` → `send_directed()`, `publish()` | ✅ 구현됨 | 최근 N개 조회 |
| 팀 토폴로지 | `OrchestrationRunStore` → `orchestration_pattern` | ✅ 구현됨 | TeamDefinition.strategy |
| Metrics (집계) | `OrchestrationRunStore.list_runs()` + `WorkItemStore` | ✅ 구현됨 | 카운트 쿼리 |

**뷰 모델 (이미 구현됨):**

```rust
// crates/opengoose-web/src/data/views/agents.rs
pub struct AgentMapView {
    pub mode_label: String,              // "Live runtime" / "Mock preview"
    pub metrics: Vec<MetricCard>,        // Active runs, Tracked agents, Success rate, Witness
    pub agents: Vec<AgentMapAgentView>,  // 에이전트 카드 목록
}

pub struct AgentMapAgentView {
    pub name: String,                    // "researcher"
    pub team: String,                    // "feature-dev"
    pub state_label: String,             // "Working" / "Idle" / "Stuck" / "Zombie"
    pub state_tone: &'static str,        // "cyan" / "neutral" / "amber" / "rose"
    pub elapsed: String,                 // "2m 14s" / "—"
}
```

**상태 색상 코딩:**

| AgentState | 색상 | tone | 의미 |
|---|---|---|---|
| Working | 🟢 cyan | `"cyan"` | 정상 작업 중 |
| Idle | ⚪ neutral | `"neutral"` | 대기 중 |
| Stuck | 🟡 amber | `"amber"` | 5분+ 무응답 — 경고 |
| Zombie | 🔴 rose | `"rose"` | 10분+ 무응답 — CancellationToken 발동 |

**SSE 업데이트:** 2초 간격으로 `#agent-map-live` 패치 (기존 dashboard.rs 패턴)

### 8.5 구현 상태 및 남은 작업

**이미 구현됨:**
- `agent_map.rs`: 라우트 + SSE 핸들러 (2초 간격 폴링)
- `agent_map.html`: 메인 템플릿 (SSE 스트림 설정)
- `agent_map_live.html`: 부분 템플릿 (테이블 형식 에이전트 목록)
- `AgentMapView`, `AgentMapAgentView`: 뷰 모델
- Mock 모드: DB 비어있을 때 샘플 3개 에이전트 표시

**남은 작업:**
1. **Message Flow 패널** — AgentMessageStore에서 최근 메시지 조회 → 타임라인 렌더링
2. **Team Topology 패널** — OrchestrationRunStore에서 실행 전략 → DAG 시각화
3. **Witness 실시간 연동** — WitnessHandle DashMap → SSE 업데이트 (현재는 DB 폴링)
4. **상태 전이 애니메이션** — CSS transition으로 카드 색상 변화 (Village Map 영감)

### 8.4 OTEL 텔레메트리

Gas Town은 OTEL 기반 관찰성을 제공한다. OpenGoose는 기존 `tracing` 크레이트에 OTEL exporter를 추가하여 Jaeger/Grafana Tempo와 통합할 수 있다:

```rust
// opengoose-cli/src/main.rs에 추가
use tracing_opentelemetry::OpenTelemetryLayer;
use opentelemetry_otlp::SpanExporter;
```

이미 `crates/opengoose-core`의 Engine에 수동 tracing span이 있으므로, exporter만 연결하면 된다.

---

## 9. 구현 로드맵

### Phase 1: 에이전트 자율성 확보 (기반)

| # | 작업 | 상태 | 의존성 | 난이도 |
|---|------|------|--------|--------|
| 1 | AgentEvent 실시간 포워딩 (runner.rs → EventBus) | ✅ 완성 | 없음 | 낮음 |
| 2 | Witness 구현 (stuck/zombie 감지) | ✅ 완성 (304줄, 14 tests) | #1 | 중간 |
| 3 | CancellationToken 통합 (에이전트 취소) | ⚠️ 부분 (Witness에만) | #2 | 낮음 |
| 4 | MCP 팀 도구 크레이트 (opengoose-team-tools) | ⚠️ 스켈레톤 (MCP+DB 연결, 도구 권한 미연동) | 없음 | 중간 |
| 5 | Agent Map 웹 뷰 | ⚠️ 기본 구조 (라우트+SSE+뷰모델, 메시지/토폴로지 패널 미완) | #2 | 중간 |
| 6 | Beads 데이터 모델 (hash_id 스키마, relations, wisp) | ✅ 스키마 완성 | 없음 | 중간 |
| 7 | Beads 알고리즘 (ready/prime/compact) | ✅ 완성 (~70 tests) | #6 | 중간 |
| 7a | hash_id 생성 함수 (SHA-256 + base36) | ❌ 미구현 (스키마만) | #6 | 낮음 |
| 7b | Wisp 생명주기 (squash/burn/promote) | ❌ 미구현 (create/purge만) | #6 | 중간 |
| 7c | Landing the Plane 프로토콜 | ❌ 미구현 | #7 | 중간 |

### Phase 2: 격리 및 머지

| # | 작업 | 상태 | 의존성 | 난이도 |
|---|------|------|--------|--------|
| 8 | per-agent Git worktree 생성/정리 | ❌ | Phase 1 완료 | 중간 |
| 9 | Extension/Permission 역할별 차등 적용 | ❌ | 없음 | 낮음 |
| 10 | "re-imagine" 머지 충돌 해결 Recipe | ❌ | #8 | 높음 |
| 11 | prollytree 전면 전환 (SQLite/Diesel 제거, KV 매핑) | ❌ | 없음 | 높음 |

### Phase 3: 규모 확장

| # | 작업 | 상태 | 의존성 | 난이도 |
|---|------|------|--------|--------|
| 12 | 20+ 에이전트 리소스 관리 (동시성, 메모리) | ❌ | Phase 2 완료 | 높음 |
| 13 | Deacon 패턴 (백그라운드 유지보수 에이전트) | ❌ | #2 | 중간 |
| 14 | OTEL 텔레메트리 통합 | ❌ | 없음 | 낮음 |
| 15 | Beads 알고리즘 prollytree 네이티브 재구현 | ❌ | #11 | 중간 |

### Phase 4: 연합 (장기)

| # | 작업 | 의존성 | 난이도 |
|---|------|--------|--------|
| 16 | 에이전트 평판 시스템 (Stamps) | #7 | 중간 |
| 17 | Yearbook Rule 구현 | #16 | 낮음 |
| 18 | 멀티 인스턴스 연합 (RemoteAgent 확장) | Phase 3 완료 | 높음 |

---

## 10. Gas Town 기능 대응표

| Gas Town 기능 | 현재 OpenGoose | v2 상태 | Phase |
|---|---|---|---|
| **Polecat (일회성 작업자)** | FanOut/Chain executor | ✅ + Witness 감독 구현 완료 | 1 |
| **Witness (헬스 순찰)** | ✅ `witness.rs` (304줄) | ✅ EventBus + 타이머 + DashMap | 1 |
| **Deacon (백그라운드 작업)** | `opengoose schedule` | ❌ Recipe 기반 자동화 | 3 |
| **Mail (에이전트 통신)** | 텍스트 파싱 ⚠️ | ⚠️ MCP 팀 도구 (스켈레톤) | 1 |
| **Git Worktree 격리** | ❌ 없음 | ❌ per-agent worktree | 2 |
| **Beads (태스크 그래프)** | ✅ hash_id 스키마 + relations + ready/prime/compact | ⚠️ hash_id 생성 함수 미구현 | 1 |
| **Refinery (머지 큐)** | ❌ 없음 | ❌ "re-imagine" Recipe | 2 |
| **Dashboard (실시간)** | ⚠️ Agent Map 기본 구조 | ⚠️ 메시지/토폴로지 패널 미완 | 1 |
| **OTEL 텔레메트리** | tracing spans | ❌ OTEL exporter | 3 |
| **Convoy (작업 번들)** | orchestration_runs | ✅ | 1 |
| **Dogs (유지보수)** | ❌ 없음 | ❌ Goose context_mgmt + Deacon | 3 |
| **Namepool (에이전트 명명)** | profile 이름 | ✅ 팀 정의에서 자동 할당 | 1 |
| **GUPP ("Hook의 작업은 실행")** | ❌ 없음 | ⚠️ Witness 구현됨, 자동 재할당 미완 | 1 |
| **Wasteland (연합)** | RemoteAgent WS 프로토콜 | ❌ 인스턴스 간 확장 | 4 |
| **Stamps (평판)** | ❌ 없음 | ❌ agent_stamps 테이블 | 4 |
| **Trust Ladder** | API key 인증 | ❌ 작업 이력 기반 신뢰 수준 | 4 |
| **Yearbook Rule** | ❌ 없음 | ❌ reviewer ≠ developer 제약 | 4 |

---

## 11. 결론

OpenGoose v2는 Gas Town의 야심을 Goose-native 방식으로 달성한다. 핵심 전략:

1. **Goose가 이미 제공하는 것은 100% 재사용** — Agent, Session, Recipe, MCP, Permission, 컨텍스트 관리
2. **격차만 최소한으로 구축** — Witness, MCP 팀 도구, Agent Map, Git worktree 격리
3. **Beads 컨셉 채택** — Hash ID, DAG 관계, ready/prime/compact, Wisp
4. **순수 Rust 단일 바이너리** — SQLite → prollytree 전환으로 외부 의존성 제거
5. **점진적 확장** — Phase 1(자율성)부터 시작, Phase 4(연합)까지 단계적 진화
6. **OpenGoose만의 차별점 유지** — 멀티채널(Discord/Slack/Telegram/Matrix) + Goose 생태계 + 웹 대시보드

Gas Town이 75k LOC의 Go 코드로 달성한 것을 OpenGoose는 **Goose 라이브러리 + 최소 오케스트레이션 레이어**로 달성한다. 이것은 절충이 아니라 전략이다 — Goose 업스트림의 모든 개선사항이 자동으로 OpenGoose에 반영된다.
