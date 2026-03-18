# OpenGoose v0.2 아키텍처

> **작성일:** 2026-03-18
> **목표:** Goose-native pull 아키텍처 + Wasteland 수준 에이전트 자율성
> **원칙:** Goose가 에이전트 작업을 한다. OpenGoose는 조율만 한다.
> **인터페이스:** CLI만. Discord/Slack/Telegram/Matrix 없음.
> **구현 상태:** `[구현]` = 로직 구현됨, `[스텁]` = 파일 존재 (빈 스텁), `[계획]` = 설계만

---

## 1. 왜 v0.2인가

v1은 21개 크레이트로 불어났고, Goose가 이미 제공하는 것들(세션, 퍼미션, 컨텍스트 관리)을 재구현했다. 아키텍처는 push 기반이었고 그 위에 pull 컨셉을 덧씌운 형태였다. prollytree Rust 크레이트에 문제가 있어서 커스텀 인메모리 구현으로 대체했는데, 이는 원래 영감을 준 Dolt 컨셉에서 점점 멀어졌다.

v0.2는 네 가지 제약으로 깨끗하게 시작한다:

1. **Goose-native** — `Agent::reply()`가 유일한 LLM 인터페이스. 래퍼 없음, 재구현 없음.
2. **Pull-only** — 모든 것이 Wanted Board를 통과. 오케스트레이터 push 없음.
3. **3개 크레이트** — `opengoose`, `opengoose-board`, `opengoose-rig`. 끝.
4. **CLI-first** — 대화형 터미널 + 헤드리스 `run` 모드. 플랫폼 게이트웨이는 나중 문제.

---

## 2. 핵심 개념

### 2.1 Pull 아키텍처

v1에서 오케스트레이터가 에이전트에게 작업을 _할당_ (push):

```
Message → Engine → Orchestrator → Agent.execute()
```

v0.2에서 에이전트가 보드에서 작업을 _가져감_ (pull):

```
CLI 입력 → Board.post(work)            // fire-and-forget
                    ↓
Rig (루핑) → Board.claim() → Goose.reply() → Board.submit()
```

CLI는 어떤 에이전트가 메시지를 처리할지 모른다. 작업을 게시할 뿐. 에이전트가 결정한다.

### 2.2 모든 것은 작업 항목이다

| 출처 (예시) | 보드에 들어가는 것 |
|-------------|-------------------|
| CLI 사용자 메시지 | WorkItem |
| `opengoose task "..."` | WorkItem |
| 에이전트가 하위 작업 생성 | WorkItem (parent: 원본) |
| 에이전트가 동료에게 위임 | WorkItem (assigned_to: 동료) |
| Cron 스케줄 실행 | WorkItem |

**WorkType enum은 없다.** 위 표는 작업 항목이 생성되는 출처를 예시한 것이지, 타입을 분류한 것이 아니다. 모든 출처가 동일한 `WorkItem` struct로 변환된다. worktree 생성 여부, 블루프린트 적용 여부, 대화인지 코드 작업인지는 rig가 실행 시점에 판단한다. 보드는 구분하지 않는다.

```rust
pub struct WorkItem {
    // === 불변 (생성 시 확정, 머지 시 변경 불가) ===
    pub id: i64,                      // AUTO INCREMENT (Board가 중앙 생성)
    pub title: String,                // 변경 불가
    pub description: String,          // 작업 내용. compact()는 시스템이 main에서 직접 수행
    pub project: Option<ProjectRef>,  // 어떤 프로젝트의 작업인지
    pub parent: Option<i64>,          // 상위 작업 (하위 작업일 때)
    pub created_by: RigId,            // 누가 올렸나 (사람도 rig)
    pub created_at: DateTime<Utc>,
    pub session_id: Option<String>,   // 대화 세션 ID (대화 메시지일 때)
    pub seq: Option<u32>,             // 세션 내 몇 번째 메시지

    // === 가변 (수명주기 중 변경됨) ===
    pub status: Status,               // 더 진행된 쪽이 이김
    pub priority: Priority,           // 더 긴급한 쪽이 이김 (에스컬레이션만)
    pub tags: Vec<String>,            // 양쪽 합집합
    pub assigned_to: Option<RigId>,   // 나중에 쓴 쪽 (위임 대상)
    pub claimed_by: Option<RigId>,    // 나중에 쓴 쪽 (현재 작업 중인 rig)
    pub updated_at: DateTime<Utc>,    // 항상 더 큰 값
    pub notes: Option<String>,        // 나중에 쓴 쪽 (메모, 보충 설명)

    // === 결과 ===
    pub result: Option<String>,       // 나중에 쓴 쪽 (완료 시 한줄 요약)
}

pub enum Status {
    Open,      // 올라왔고 아무도 안 가져감
    Claimed,   // rig가 작업 중
    Done,      // 끝남
    Stuck,     // 문제 생김, 사람이 봐야 함
    Abandoned, // 포기
}
// 순서: Done > Abandoned > Stuck > Claimed > Open

pub enum Priority {
    P0,  // 긴급
    P1,  // 보통
    P2,  // 낮음
}
// 순서: P0 > P1 > P2 (에스컬레이션만, 내려가지 않음)
```

**상태 전이:**

```
Open → Claimed        rig가 claim
Claimed → Done        rig가 완료
Claimed → Open        unclaim, crash 복구, timeout
Claimed → Stuck       CI 2라운드 초과, stuck timeout
Stuck → Open          /retry
Stuck → Abandoned     /abandon
Open → Abandoned      /abandon
```

대화 메시지는 Open → Claimed → Done이 1초 안에 일어남. 사용자는 모름.

### 2.3 프로젝트 컨텍스트

Rig와 프로젝트는 독립적인 축이다. Rig는 프로젝트에 대해 모른다 — 작업 항목이 프로젝트를 알고 있고, rig는 claim한 작업의 컨텍스트를 받아서 사용할 뿐.

```rust
pub struct ProjectRef {
    pub name: String,        // "myapp"
    pub path: PathBuf,       // ~/dev/myapp
}
```

**프로젝트 컨텍스트 = "코드 작업할 때 어디서 하는지".** 대화가 프로젝트에 관한 건지 범용인지는 LLM이 판단. 우리가 분류하지 않는다.

```bash
# git repo 안에서 실행 → 프로젝트 컨텍스트 자동 설정
$ cd ~/dev/myapp && opengoose
> JWT 만료 처리가 어떻게 돼 있어?     # → project: Some("myapp"), 코드 읽기 가능
> 일반적으로 rate limiting은 뭐가 있어? # → project: Some("myapp"), LLM이 범용으로 답변
> /task "rate limiting 추가"            # → project: Some("myapp"), worktree 생성

# git repo 밖에서 실행 → 프로젝트 없음
$ cd ~ && opengoose
> JWT 만료 처리 일반적인 방법이 뭐야?   # → project: None, 범용 답변
> /task "뭔가 구현해줘"                 # → 에러: "프로젝트를 지정해주세요"

# 명시적 지정
$ opengoose --project ~/dev/myapp

# 대화 중 전환
> /project ~/dev/backend
```

Rig 입장에서:

```
Rig "researcher" (L2, 어떤 프로젝트든 작업 가능)
  ├─ #12 claim (project: myapp)   → working_dir = ~/dev/myapp
  ├─ #15 claim (project: backend) → working_dir = ~/dev/backend
  └─ #18 claim (project: None)    → worktree 없음, 범용
```

### 2.4 대화는 작업 항목이 아니다 — Operator와 Worker

**듀얼 패스 아키텍처:** 대화와 작업은 다른 경로를 탄다.

```
대화 (hot path):
  User → Operator.chat(msg) → Agent.reply(영속 세션) → stream 응답
                                  Board 안 거침. WorkItem 생성 안 됨.

작업 (cold path):
  User → Board.post(task) → Worker.pull() → claim → Agent.reply(작업 세션) → submit
```

왜 대화를 Board에서 분리하는가:
- "오늘 날씨 어때?"가 WorkItem #47로 등록되면 안 된다
- 대화에는 조율할 것이 없다 (1:1, 누가 응답할지 경쟁 불필요)
- Board의 가치는 **조율**이다 — 대화에서는 낭비
- 영속 세션을 유지해야 prompt caching이 보장된다 (§ 5.6)

Operator는 Board에 **접근 권한은 있다** (읽기, 태스크 생성). Board를 **통과하지 않을 뿐이다.**

```
> 오늘 날씨 어때?
  → Operator가 직접 응답 (Board 무관)

> auth 모듈에 rate limiting 추가해줘
  → Operator가 대화로 스펙 확인
  → Operator가 board__create_task 호출 → Board에 태스크 생성
  → Worker가 pull → 작업 시작

> 그거 어떻게 되고 있어?
  → Operator가 board__read_board 호출 → 상태 보고
```

Goose의 `SessionManager`가 Operator의 대화 이력을 내부적으로 관리. Board는 작업 항목 수명주기만 추적.

### 2.5 블루프린트 패턴

> 설계 배경: [REFERENCE.md § 7 — Production Agent Systems](REFERENCE.md#7-production-agent-systems-open-swe--stripe--ramp--coinbase)

복잡한 작업은 결정론적 노드 + 에이전트 노드를 교차 사용:

```
사용자가 작업 게시
  → [결정론적] 티켓 파싱, 컨텍스트 사전 수집 (AGENTS.md 읽기, 참조 fetch)
  → [에이전트] 구현 계획 (Goose Agent::reply)
  → [결정론적] git worktree 생성, 계획 구조 검증
  → [에이전트] 구현 (worktree 안에서 Goose Agent::reply)
  → [결정론적] lint 실행, 테스트 실행
  → [에이전트] 실패 수정 (제한: 최대 2라운드)
  → [결정론적] 커밋, PR 생성, worktree 정리
```

결정론적 노드는 토큰을 절약하고 예측 가능하다. 에이전트 노드는 열린 추론을 담당.

---

## 3. 크레이트 구조

```
opengoose-v0.2/
├── Cargo.toml                    # [스텁] 워크스페이스
├── crates/
│   ├── opengoose/                # 바이너리 — CLI (대화형 + 헤드리스)
│   │   └── src/
│   │       ├── main.rs           # [스텁] 진입점
│   │       ├── cli.rs            # [계획] 대화형 REPL (사용자 입력 → 보드)
│   │       ├── run.rs            # [계획] 헤드리스 모드 (레시피 → 보드 → 실행 → 종료)
│   │       └── status.rs         # [계획] 보드 상태 표시
│   │
│   ├── opengoose-board/          # [스텁] Wanted Board + Beads + 데이터
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── board.rs          # WantedBoard: post/claim/submit/merge
│   │       ├── work_item.rs      # WorkItem, Status, Priority
│   │       ├── store.rs          # 커스텀 CoW BTreeMap 스토어 (Dolt 영감)
│   │       ├── branch.rs         # 에이전트별 브랜치 격리 + 3-way merge
│   │       ├── merge.rs          # 셀 레벨 충돌 해결
│   │       ├── beads.rs          # ready() / prime() / compact()
│   │       ├── stamps.rs         # Stamps + Trust Ladder + Yearbook Rule
│   │       └── relations.rs      # 의존성 그래프 (blocks, depends_on)
│   │
│   └── opengoose-rig/            # [스텁] Agent Rig (Strategy 패턴)
│       └── src/
│           ├── lib.rs
│           ├── rig.rs            # Rig<M>: Strategy context, process()
│           ├── work_mode.rs      # WorkMode trait + ChatMode + TaskMode
│           ├── agent_setup.rs    # 공유: Agent + Board 와이어링 (팩토리)
│           ├── memory.rs         # Layer 2: 경험 기억 (remember/recall/tree)
│           ├── worktree.rs       # Git worktree 관리
│           ├── portless.rs       # Portless 스타일 네임드 URL 할당
│           ├── witness.rs        # Stuck/zombie 감지
│           ├── middleware.rs     # Before/after 훅 (컨텍스트 사전 수집, 안전망)
│           └── mcp_tools.rs      # MCP 서버: 에이전트용 보드 + 메모리 도구
```

### 3.1 의존성 그래프

```
opengoose-board           (OpenGoose 의존성 없음, serde/chrono/tokio만)
       ↑
opengoose-rig             (의존: board, goose)
       ↑
opengoose                 (의존: board, rig — CLI 바이너리)
```

### 3.2 각 크레이트가 하지 않는 것

| 크레이트 | 하지 않는 것 |
|----------|-------------|
| **board** | LLM 호출, 세션 관리, 도구 실행, 플랫폼 인식 |
| **rig** | 메시지 라우팅, 플랫폼 관리, 데이터 저장, 텍스트 프로토콜 파싱 |
| **opengoose** | 비즈니스 로직 포함 (CLI + 와이어링만) |

---

## 4. 데이터 레이어

### 4.1 데이터 계층: Global / Per-Rig / Per-Project

데이터가 속하는 곳에 따라 세 계층으로 나뉜다:

```
┌─ Global (인스턴스 전체) ─────────────────────────┐
│  Rig 등록 (id, recipe, HOP URI)                  │
│  Stamps + Trust (rig의 평판은 프로젝트 무관)     │
│  전역 설정                                       │
└──────────────────────────────────────────────────┘

┌─ Per-Rig (rig 자체에 속함) ──────────────────────┐
│  현재 상태 (idle, working, stuck)                │
│  작업 이력 (뭘 했고 어떻게 끝났는지)             │
│  respawn 횟수 (circuit breaker)                  │
│  통계 (완료율, 평균 소요 시간, 실패율)           │
└──────────────────────────────────────────────────┘

┌─ Per-Project (프로젝트 단위) ────────────────────┐
│  Work items (project 컬럼으로 필터링)            │
│  Relations (작업 간 의존성)                      │
│  프로젝트 컨텍스트 (AGENTS.md 등)                │
│  경험 기억 (파일 기반, DB 밖)                    │
└──────────────────────────────────────────────────┘
```

| 데이터 | 스코프 | 이유 |
|--------|--------|------|
| Stamps, Trust | **Global** | 연구자의 능력은 어디서 일했든 동일 |
| Rig 설정 (recipe, auto_start) | **Global** | 프로젝트 무관 |
| Rig 상태 + 이력 | **Per-Rig** | rig 자체에 누적, 프로젝트 단위가 아님 |
| Work items, Relations | **단일 DB** (project 컬럼) | 글로벌 정수 ID, project로 필터링 |
| CoW branches, Commit log | **단일 DB** | 브랜치 격리는 인메모리 CowStore에서 |
| 대화 이력 | **Goose Session** | Goose가 관리, 우리가 신경 안 씀 |
| 경험 기억 (메모리 파일) | **Per-Rig + Per-Project** | Layer 2, § 4.5 참조 |

**Per-Rig 데이터** (Gas Town의 Agent Bead, Wasteland의 completions에서 참조):

```rust
pub struct RigState {
    // 현재 상태
    pub status: RigStatus,           // Idle, Working, Stuck, Zombie
    pub current_work: Option<i64>,  // 지금 뭘 하고 있는지 (work item id)
    pub current_project: Option<String>,

    // 이력 (프로젝트 무관, rig에 누적)
    pub completions: Vec<Completion>,
    pub total_completed: u32,
    pub total_failed: u32,
    pub avg_duration: Duration,

    // 건강 (Gas Town Witness의 respawn counter에서 참조)
    pub respawn_count: u32,          // circuit breaker
    pub last_activity: DateTime<Utc>,
}

pub struct Completion {
    pub work_item: i64,              // work item id
    pub project: Option<String>,     // 어떤 프로젝트에서 했는지
    pub exit_type: ExitType,         // Completed, Failed, Escalated
    pub duration: Duration,
    pub completed_at: DateTime<Utc>,
}
```

**저장소 구조:**

```
~/.opengoose/
├── config.yaml                    # 전역 설정
├── rigs.yaml                      # Global: rig 등록 정보
├── opengoose.db                   # 단일 SQLite — 모든 데이터
│                                  #   work_items (project 컬럼으로 구분)
│                                  #   stamps, trust, rig 이력
│                                  #   cow_store, commit_log
│
├── rigs/                          # Per-Rig 데이터
│   ├── researcher-01/
│   │   └── memory/                # Layer 2: 경험 기억 (§ 4.5)
│   │       ├── MEMORY.md          # 큐레이션된 장기 기억 (evergreen)
│   │       ├── daily/             # 일간 로그 (30일 반감기)
│   │       │   └── 2026-03-18.md
│   │       └── TREE.md            # 자동 생성 메모리 트리 (read-only)
│   └── developer-01/
│       └── memory/
│
└── projects/                      # Per-Project 파일 데이터 (DB 밖)
    ├── myapp/
    │   └── memory/                # Layer 2: 프로젝트 공유 기억 (§ 4.5)
    │       ├── MEMORY.md
    │       ├── daily/
    │       └── TREE.md
    │
    └── backend/
        └── memory/
```

**Board:**

```rust
pub struct Board {
    store: CowStore,                              // WorkItem만 — 브랜치/머지 대상
    relations: HashMap<i64, Vec<Relation>>,        // 직접 관리 (브랜치 격리 불필요)
    stamps: Vec<Stamp>,                            // 직접 관리 (졸업앨범 규칙으로 충돌 없음)
    db: Option<SqlitePool>,                        // Phase 4에서 추가
}

impl Board {
    // Trust는 전역 (모든 프로젝트의 stamp 합산)
    fn trust_level(&self, rig_id: &RigId) -> TrustLevel {
        self.compute_trust(rig_id)
    }

    // ready()는 project 컬럼으로 필터링
    fn ready(&self, rig_id: &RigId, project: Option<&str>) -> Vec<WorkItem> {
        self.store.ready(rig_id, project)  // project=None이면 전체 조회
    }
}
```

**왜 CowStore에 WorkItem만 넣는가:**
- **WorkItem** — 여러 rig가 동시에 status, claimed_by, result를 변경. 충돌 가능 → 브랜치 격리 필요.
- **Stamps** — 졸업앨범 규칙(`stamped_by != target_rig`)으로 동시 충돌 불가 → main에서 직접 관리.
- **Relations** — 작업 생성 시 설정, 이후 변경 드묾 → main에서 직접 관리.
- 나중에 동시성 문제가 생기면 Stamps/Relations를 별도 CowStore로 분리 가능.

### 4.2 스토리지: 커스텀 CoW 스토어 (prollytree 크레이트 아님)

prollytree Rust 크레이트 (v0.3.2-beta)는 v1 개발 중 문제가 있었다. v0.2는 Dolt의 prolly tree 컨셉에서 영감받은 커스텀 구현을 사용한다:

**핵심: 콘텐츠 주소 지정이 가능한 Copy-on-Write BTreeMap**

```rust
pub struct CowStore {
    data: Arc<BTreeMap<i64, WorkItem>>,  // key: 작업 ID, value: 작업 항목
    root_hash: OnceCell<[u8; 32]>,       // SHA-256, 캐시됨, 변이 시 무효화
}
```

유지하는 Dolt 컨셉:
- **콘텐츠 주소 지정** — 무결성 검증을 위한 SHA-256 루트 해시
- **O(1) 브랜칭** — Arc clone, 첫 쓰기 시 CoW
- **O(d) diff** — 브랜치 간 변경된 키만 비교
- **3-way merge** — Base vs source vs dest, 셀 레벨 해결
- **커밋 로그** — 감사 추적을 위한 append-only 해시 체인

의도적으로 생략하는 것:
- 확률적 청킹 (인메모리 스토어에는 불필요)
- 머클 트리 경로 증명 (연합 전까지 불필요)
- 온디스크 포맷 (내구성은 SQLite, 연산은 CoW 스토어)

### 4.3 영속성 전략 `[계획]`

> SQLite 의존성은 아직 Cargo.toml에 없다. Phase 1은 인메모리만으로 Board API를 완성한다. 영속성은 Phase 4에서 추가. 아래는 목표 아키텍처 (Phase 4).

**원칙: 재시작해도 상태 유지** (Beads/Wasteland 패턴: append-only 장부, 즉시 롤백)

```
┌─ 인메모리 (빠름) ─────────────────────┐
│  CowStore (BTreeMap, Arc CoW)         │
│  - WorkItem만 (브랜치/머지 대상)     │
│  - 에이전트별 브랜치                  │
│  - 루트 해시 캐싱                     │
│  Board 직접 관리                      │
│  - Relations (HashMap)                │
│  - Stamps (Vec)                       │
└───────────────┬───────────────────────┘
                │ 주기적 스냅샷 + WAL
┌───────────────▼───────────────────────┐
│  SQLite (내구성)                      │
│  - 커밋 로그 (해시 체인)             │
│  - 스냅샷 (주기적 전체 덤프)         │
│  - 뮤테이션 로그 (복구용 WAL)        │
│  - Stamp 이력                        │
│  - 세션 메타데이터                    │
└───────────────────────────────────────┘
```

시작: 최신 스냅샷 로드 → WAL 리플레이 → 준비 완료.
개발/테스트 시 초기화: `opengoose --clean` 플래그로 DB 삭제 후 시작.

### 4.4 브랜치 수명주기

**순수 스냅샷 모델 (Dolt/Git과 동일):** 브랜치는 분기 시점의 데이터만 본다. main에 이후 추가된 항목은 merge할 때 합쳐진다. `ready()`와 `prime()`은 항상 main에서 호출되므로 브랜치에서 main을 볼 필요가 없다. `board__create_task`로 만든 하위 작업도 브랜치에 쓰이고, merge 후에 다른 rig에게 보인다.

```
Rig 생성
  → board.branch("rig-researcher")     // main의 Arc clone, O(1)
                                       // 이 시점의 스냅샷만 보임

Rig가 작업 claim
  → 브랜치에 쓰기                      // CoW: 첫 쓰기 시 BTreeMap clone 발생
  → board.commit("claimed #42")         // 루트 해시 스냅샷

Rig가 작업 완료
  → board.merge(branch, main)          // 3-way: base(분기 시점) vs branch vs main
  → 충돌? → 셀 레벨 해결               // 하위 작업도 이때 main에 반영
  → 커밋 로그 항목 (해시 체인)

Rig 실패
  → board.drop(branch)                 // main은 영향 없음
```

### 4.5 메모리 레이어

> 설계 배경: [REFERENCE-memory.md](REFERENCE-memory.md) — OpenClaw, QMD, Letta Code

에이전트 "기억"은 세 가지 다른 종류가 있고, 각각 다른 시스템이 소유한다:

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: 대화 기억                                          │
│   소유: Goose SessionManager                                │
│   범위: 현재 세션                                           │
│   수명: 세션 종료 시 압축 (80% 임계값)                      │
│   v0.2: 건드리지 않음. pre-compaction flush만 훅.           │
├─────────────────────────────────────────────────────────────┤
│ Layer 2: 경험 기억                                          │
│   소유: Rig (에이전트가 도구로 읽기/쓰기)                   │
│   범위: per-rig + per-project                               │
│   수명: 일간 로그 30일 반감기, MEMORY.md는 영구             │
│   v0.2: Phase 2에서 구현.                                   │
├─────────────────────────────────────────────────────────────┤
│ Layer 1: 조직 기억                                          │
│   소유: Board (시스템이 관리)                                │
│   범위: per-project                                         │
│   수명: ready/prime/compact 수명주기                        │
│   v0.2: 이미 설계됨 (§ 7 Beads 알고리즘). 변경 없음.       │
└─────────────────────────────────────────────────────────────┘
```

**왜 3개 레이어인가:**

| | Layer 1 (조직) | Layer 2 (경험) | Layer 3 (대화) |
|--|----------------|----------------|----------------|
| 뭘 기억 | 작업, 상태, 의존성, 결과 | 학습, 패턴, 선호, 암묵지 | 현재 대화 맥락 |
| 누가 쓰나 | 시스템 (board__update) | 에이전트 (board__remember) | Goose (자동) |
| 예시 | "PR #42가 머지됨" | "channel을 쓴 이유는 race condition" | "방금 유저가 rate limiting 요청" |
| 검색 | prime() 요약 주입 | board__recall (시맨틱) | Goose 컨텍스트 윈도우 |
| 감쇠 | compact() (30일) | 일간 로그 반감기 (30일) | 압축 (80% 임계값) |

Layer 1의 `compact()`와 Layer 2의 시간 감쇠가 같은 30일을 사용하지만 **다른 것에 적용**:
- `compact()`: **작업 항목의 상세 설명**을 AI 요약으로 축소 (id/title/status 보존)
- 시간 감쇠: **일간 메모리 로그의 검색 점수**를 낮춤 (파일 자체는 삭제 안 함)

#### 4.5.1 Layer 2 상세: 경험 기억

**저장 구조:**

```
~/.opengoose/
├── rigs/{rig-id}/memory/           # Per-Rig 경험 기억
│   ├── MEMORY.md                   # 큐레이션된 장기 기억 (evergreen, 감쇠 없음)
│   ├── daily/                      # 일간 로그 (시간 감쇠 적용)
│   │   ├── 2026-03-18.md
│   │   └── 2026-03-17.md
│   └── TREE.md                     # 자동 생성 메모리 트리 (read-only)
│
├── projects/{name}/memory/         # Per-Project 공유 기억
│   ├── MEMORY.md                   # 프로젝트 공유 장기 기억
│   ├── daily/
│   │   └── 2026-03-18.md
│   └── TREE.md
```

**2계층 메모리 (OpenClaw 참조):**

| 계층 | 파일 | 감쇠 | 용도 |
|------|------|------|------|
| **큐레이션** | `MEMORY.md` | 없음 (evergreen) | 에이전트가 중요하다고 판단한 영구 지식 |
| **일간 로그** | `daily/YYYY-MM-DD.md` | 30일 반감기 | 작업 중 발견한 것, 시도한 접근, 결정 이유 |

에이전트는 일간 로그에 자유롭게 쓰고, 중요한 것을 MEMORY.md로 승격한다.

**Progressive disclosure (Letta MemFS 참조):**

`TREE.md`는 자동 생성되는 메모리 "목차". 내용 없이 파일명과 첫 줄(설명)만:

```markdown
# Memory Tree (auto-generated, read-only)

## Per-Rig: researcher-01
- MEMORY.md: "이 프로젝트의 auth 모듈 패턴과 선호"
- daily/2026-03-18.md: "rate limiting 조사 중 발견한 것"
- daily/2026-03-17.md: "JWT 갱신 로직 리팩토링 기록"

## Per-Project: myapp
- MEMORY.md: "프로젝트 컨벤션, 아키텍처 결정"
- daily/2026-03-18.md: "CI 설정 변경 기록"
```

이 트리가 `prime()`에 포함된다 (수백 토큰). 에이전트는 트리를 보고 필요한 파일만 `board__recall`로 로드. **1-2K 토큰 제한을 구조적으로 극복.**

**스코핑 규칙:**

| 스코프 | 쓰기 | 읽기 |
|--------|------|------|
| Per-Rig | 해당 rig만 | 해당 rig만 |
| Per-Project | 모든 rig (해당 프로젝트 작업 시) | 모든 rig (해당 프로젝트 작업 시) |

Per-Rig 메모리는 rig의 "개인 노트". Per-Project 메모리는 "팀 위키".

Rig가 교체되어도 per-project 메모리는 남아있다 → Wasteland "메모리는 에이전트 밖에" 원칙 유지.

#### 4.5.2 경험 기억 도구 (Platform Extension)

```
board__remember    → 메모리에 기록
board__recall      → 메모리에서 검색
board__memory_tree → 메모리 트리 조회 (TREE.md 내용)
```

**`board__remember`:**

```rust
pub struct RememberInput {
    pub content: String,         // 기록할 내용 (자유 형식 마크다운)
    pub scope: MemoryScope,      // Rig | Project
    pub target: MemoryTarget,    // Daily | Curated
}

pub enum MemoryScope {
    Rig,       // ~/.opengoose/rigs/{rig-id}/memory/
    Project,   // ~/.opengoose/projects/{name}/memory/
}

pub enum MemoryTarget {
    Daily,     // daily/YYYY-MM-DD.md에 append
    Curated,   // MEMORY.md에 append (또는 편집)
}
```

에이전트가 호출:
```
board__remember("channel을 쓴 이유는 race condition 때문", scope=Project, target=Curated)
→ ~/.opengoose/projects/myapp/memory/MEMORY.md에 추가
```

**`board__recall`:**

```rust
pub struct RecallInput {
    pub query: String,           // 검색어
    pub scope: RecallScope,      // Rig | Project | All
    pub limit: usize,            // 최대 결과 수 (기본: 5)
}

pub enum RecallScope {
    Rig,       // 이 rig의 메모리만
    Project,   // 이 프로젝트의 공유 메모리만
    All,       // Rig + Project (기본)
}

pub struct RecallResult {
    pub content: String,         // 매칭된 내용 (~700자)
    pub source: String,          // 파일 경로
    pub score: f32,              // 관련도 점수
    pub age_days: u32,           // 기록 후 경과일
}
```

**검색 구현 (단계적):**

| Phase | 검색 방식 | 복잡도 |
|-------|----------|--------|
| Phase 2 | **BM25 텍스트 검색** (SQLite FTS5) | 낮음 |
| Phase 5+ | **하이브리드** (BM25 + 벡터, QMD 사이드카) | 높음 |

Phase 2에서는 SQLite FTS5로 충분하다. 메모리 파일이 수백~수천 개 수준이면 키워드 검색만으로도 관련 결과를 찾을 수 있다. QMD 사이드카는 메모리가 대량으로 쌓인 후 시맨틱 검색이 필요할 때.

#### 4.5.3 Pre-compaction Flush (OpenClaw 참조)

Goose의 자동 압축 직전에 에이전트에게 기억 기록 기회를 주는 메커니즘:

```
Goose 컨텍스트가 ~75% 찼을 때 (압축 80% 미만):
  1. Rig가 AgentEvent 스트림에서 토큰 사용량 모니터링
  2. 임계값 도달 시 시스템 메시지 주입:
     "세션 컨텍스트가 곧 압축됩니다. 중요한 학습이나 결정을
      board__remember로 기록하세요. 기록할 것이 없으면 무시하세요."
  3. 에이전트가 board__remember 호출 (또는 무시)
  4. Goose 자동 압축 진행 (80% 임계값)
```

```rust
impl Rig {
    /// Agent::reply() 스트림에서 토큰 사용량 추적
    async fn check_flush_needed(&self, event: &AgentEvent) -> bool {
        if let AgentEvent::Usage(usage) = event {
            let ratio = usage.total_tokens as f32 / self.context_limit as f32;
            // 75%에서 flush 트리거 (압축 80% 전에)
            // 세션당 1회만
            ratio > 0.75 && !self.flush_triggered
        } else {
            false
        }
    }

    /// Flush 메시지 주입
    async fn inject_flush_prompt(&mut self) {
        self.flush_triggered = true;
        let msg = Message::system(FLUSH_PROMPT);
        // Goose 세션에 시스템 메시지로 주입
        self.agent.inject_message(msg).await;
    }
}
```

**왜 75%인가:** Goose 압축이 80%에서 트리거된다. Flush는 그 전에 발생해야 에이전트가 기록할 시간이 있다. 5% 여유 ≈ 수천 토큰 ≈ 충분한 기록 시간.

**`GOOSE_AUTO_COMPACT_THRESHOLD`를 바꾸면?** Flush 임계값도 연동: `flush_threshold = compact_threshold - 0.05`.

#### 4.5.4 시간 감쇠

일간 로그에만 적용. MEMORY.md와 TREE.md는 감쇠 면제 (evergreen).

```rust
fn decayed_recall_score(result: &RecallResult, half_life_days: f32) -> f32 {
    if result.source.contains("MEMORY.md") {
        return result.score;  // evergreen
    }
    let decay = 0.5_f32.powf(result.age_days as f32 / half_life_days);
    result.score * decay
}
```

기본 반감기: 30일 (Wasteland stamps, OpenClaw과 동일).

**감쇠의 효과:**
- 오늘 기록한 학습: 100% 점수
- 7일 전: ~84%
- 30일 전: 50%
- 90일 전: 12.5%

중요한 학습은 에이전트가 MEMORY.md로 승격하면 영구 보존.

#### 4.5.5 Rig.execute()와 메모리 통합

§ 5.1의 `Rig.execute()` 플로우에 메모리가 어떻게 끼어드는지:

```
1. 브랜치 생성                    (기존)
2. pre_hydrate (AGENTS.md 등)     (기존)
3. prime (보드 상태 요약)          (기존)
4. 메모리 트리 로드 (TREE.md)     ← NEW: Layer 2
5. Goose 에이전트 생성/재사용     (기존)
6. Goose로 실행                   (기존)
   ├─ 에이전트가 board__remember 호출 가능  ← NEW: Layer 2
   ├─ 에이전트가 board__recall 호출 가능    ← NEW: Layer 2
   └─ 75% 컨텍스트 시 flush 트리거         ← NEW: Layer 3→2 브릿지
7. post_execute (lint, test)      (기존)
8. 브랜치 머지                    (기존)
```

**Step 4** — `prime()`에 메모리 트리 "목차"가 추가됨:

```rust
pub fn prime(&self, rig_id: &RigId) -> String {
    let board_summary = self.board_prime(rig_id);    // 기존: 보드 상태
    let memory_tree = self.memory_tree(rig_id);       // NEW: TREE.md 내용
    format!("{}\n\n{}", board_summary, memory_tree)
}
```

**Step 6** — 에이전트가 실행 중 자유롭게 `board__remember`/`board__recall` 사용.

**Flush** — Goose 세션이 75% 차면 기록 기회. 이후 Goose 자동 압축(80%).

#### 4.5.6 Layer 간 데이터 흐름 (없음 = 격리)

```
Layer 1 (Board) ←→ Layer 2 (Memory): 직접 연결 없음
  - prime()은 Layer 1만 요약
  - board__recall은 Layer 2만 검색
  - 에이전트가 판단하여 Layer 1의 작업 결과를 Layer 2에 기록할 수 있지만
    시스템이 자동으로 하지 않음

Layer 2 (Memory) ←→ Layer 3 (Session): pre-compaction flush
  - Layer 3이 압축되기 전에 Layer 2에 기록 기회
  - 이것이 유일한 레이어 간 브릿지

Layer 1 (Board) ←→ Layer 3 (Session): 없음 (독립)
  - Goose가 세션을, Board가 작업을 각각 독립 관리
```

**의도적 결정: 자동 동기화를 하지 않는다.** 에이전트가 뭘 기억할 가치가 있는지 판단한다. 시스템이 모든 것을 자동 기록하면 노이즈가 쌓인다.

### 4.6 충돌 해결 (Beads + Dolt 영감)

3-way merge: base(분기 시점) vs source(브랜치) vs dest(main)를 비교.

**4가지 규칙 (Beads 머지 드라이버와 동일):**

| # | 상황 | 규칙 | 적용 필드 |
|---|------|------|----------|
| 1 | 한쪽만 고침 | 고친 쪽 반영 | 모든 필드 (Dolt 기본 동작) |
| 2 | 스칼라 양쪽 고침 | 나중에 쓴 쪽 (`updated_at` 비교) | notes, assigned_to, claimed_by, result |
| 3 | 배열 양쪽 고침 | 합치기 (union, 중복 제거) | tags |
| 4 | status / priority | 더 높은 쪽 | status (Done > Claimed), priority (P0 > P1) |

불변 필드(id, title, description, project, parent, created_by, created_at, session_id, seq)는 머지 대상이 아니다 — 양쪽이 같은 값을 가지므로 충돌 자체가 불가능. `description`은 `compact()` 시스템 작업으로만 변경되며, 이는 main에서 직접 수행되어 머지를 거치지 않는다.

같은 항목의 다른 필드를 양쪽이 변경 → 둘 다 적용 (충돌 아님). Dolt의 셀 레벨 머지와 동일.

---

## 5. Rig 아키텍처

### 5.1 Strategy 패턴: WorkMode

Operator(대화)와 Worker(작업)는 **생성 로직을 공유하지만 런타임 행동이 다르다.** Strategy 패턴으로 이 차이를 캡슐화한다.

```rust
/// Strategy: 메시지 처리 전후의 행동을 캡슐화.
/// 세션 관리와 Board 상호작용이 달라지는 지점.
pub trait WorkMode: Send + Sync {
    /// 어떤 Goose 세션을 사용할지 결정.
    fn session_for(&self, input: &WorkInput) -> String;

    /// Agent 실행 전 (기본: no-op)
    async fn pre(&self, board: &Mutex<Board>, id: &RigId, input: &WorkInput) -> Result<()> {
        Ok(())
    }

    /// Agent 실행 후 (기본: no-op)
    async fn post(&self, board: &Mutex<Board>, id: &RigId, input: &WorkInput) -> Result<()> {
        Ok(())
    }
}

/// ChatMode: Operator용. 영속 세션, Board 안 거침.
pub struct ChatMode {
    session_id: String,
}

impl WorkMode for ChatMode {
    fn session_for(&self, _: &WorkInput) -> String {
        self.session_id.clone()  // 항상 같은 세션 → prompt cache 보장
    }
    // pre/post: default no-op
}

/// TaskMode: Worker용. 작업당 세션, claim/submit.
pub struct TaskMode;

impl WorkMode for TaskMode {
    fn session_for(&self, input: &WorkInput) -> String {
        format!("task-{}", input.work_id())
    }

    async fn pre(&self, board: &Mutex<Board>, id: &RigId, input: &WorkInput) -> Result<()> {
        board.lock().await.claim(input.work_id(), id)?;
        Ok(())
    }

    async fn post(&self, board: &Mutex<Board>, id: &RigId, input: &WorkInput) -> Result<()> {
        board.lock().await.submit(input.work_id(), id)?;
        Ok(())
    }
}
```

### 5.2 Rig\<M> — Strategy를 사용하는 Context

```rust
pub struct Rig<M: WorkMode> {
    pub id: RigId,
    pub recipe: String,               // Goose 레시피 이름 (v1의 "profile" 대체)
    pub trust_level: TrustLevel,      // L1..L3 (stamps에서 파생)
    agent: Agent,                     // Goose Agent
    board: Arc<Mutex<Board>>,         // 보드에 대한 공유 참조
    mode: M,                          // Strategy: ChatMode 또는 TaskMode
    cancel: CancellationToken,
}

/// 타입 별칭 — 역할을 명시적으로.
pub type Operator = Rig<ChatMode>;
pub type Worker = Rig<TaskMode>;
```

**공유 로직** — `process()`는 모든 모드에서 동일:

```rust
impl<M: WorkMode> Rig<M> {
    /// 공유 메시지 처리 파이프라인.
    /// session config, stream 처리, flush 모니터링 — 한 곳에서 관리.
    pub async fn process(&self, input: WorkInput) -> Result<impl Stream<Item = AgentEvent>> {
        self.mode.pre(&self.board, &self.id, &input).await?;

        let session_id = self.mode.session_for(&input);
        let config = SessionConfig { id: session_id, .. };
        let message = input.into_message();
        let stream = self.agent.reply(message, config, Some(self.cancel.clone())).await?;

        // TODO: stream 완료 후 mode.post() 호출
        Ok(stream)
    }
}
```

**Operator 전용** — Board를 거치지 않는 직접 대화:

```rust
impl Operator {
    /// 사용자와 직접 대화. Board를 통과하지 않음.
    pub async fn chat(&self, input: &str) -> Result<impl Stream<Item = AgentEvent>> {
        self.process(WorkInput::chat(input)).await
    }
}
```

**Worker 전용** — Board에서 pull하는 루프:

```rust
impl Worker {
    /// Pull loop. Operator에는 이 메서드가 없음 — 컴파일타임 보장.
    pub async fn run_pull_loop(&mut self) {
        loop {
            tokio::select! {
                _ = self.board.lock().await.wait_for_claimable() => {
                    let ready = self.board.lock().await.ready();
                    if let Some(work) = ready.first() {
                        self.process(WorkInput::task(work)).await;
                    }
                }
                _ = self.cancel.cancelled() => break,
            }
        }
    }
}
```

### 5.3 Worker.execute() — 작업 실행 상세

Worker가 Board에서 claim한 작업의 전체 실행 플로우:

```rust
impl Worker {
    async fn execute(&mut self, work: WorkItem) {
        // 1. 격리를 위한 브랜치 생성
        let branch = self.board.lock().await.branch(&self.id);

        // 2. 컨텍스트 사전 수집 (결정론적 — LLM 호출 없음)
        let context = self.middleware.pre_hydrate(&work).await;

        // 3. Prime (Beads 요약 + 메모리 트리)
        let prime = self.board.lock().await.prime(&self.id);

        // 4. 선택적: 코드 작업용 git worktree
        if work.needs_code_isolation() {
            self.worktree = Some(WorktreeHandle::create(&self.id, &work)?);
        }

        // 5. process() 호출 (Strategy가 claim/submit 처리)
        let input = work.to_work_input(&context, &prime);
        self.process(input).await;

        // 6. 실행 후 훅 (결정론적 — lint, test, PR)
        self.middleware.post_execute(&work).await;

        // 7. 브랜치 머지
        self.board.lock().await.merge_branch(&branch);

        // 8. TREE.md 재생성 (메모리 변경 시)
        self.memory.rebuild_tree(&self.id, work.project.as_ref());

        // 9. worktree 정리
        if let Some(wt) = self.worktree.take() {
            wt.cleanup();
        }
    }
}
```

**Operator.chat()과 Worker.execute()의 비교:**

| 단계 | Operator | Worker |
|------|----------|--------|
| 입력 | 사용자 메시지 직접 | Board에서 claim |
| 세션 | 영속 (ChatMode) | 작업당 생성 (TaskMode) |
| process() | 동일 | 동일 |
| 전후 훅 | 없음 | claim/submit + middleware |
| 브랜치 | 없음 | 격리 브랜치 + 머지 |
| worktree | 없음 | 코드 작업 시 생성 |

### 5.4 Goose 통합 (최소한)

Rig\<M>이 Goose와 하는 것은 정확히 세 가지:

1. **Agent 생성** — Recipe로부터 (v1의 profile 대체)
2. **`agent.reply()` 호출** — `process()`에서 Strategy를 통해
3. **`AgentEvent` 스트림 소비** — 결과와 liveness 확인

나머지 전부 (MCP 도구 디스패치, 컨텍스트 관리, 에러 복구, 프로바이더 추상화)는 Goose의 몫.

### 5.5 미들웨어 훅

> 설계 배경: [REFERENCE.md § 7 — 미들웨어 훅](REFERENCE.md#7-프로덕션-에이전트-시스템-open-swe--stripe--ramp--coinbase)

```rust
pub trait Middleware: Send + Sync {
    /// Rig 시작 시 1회 초기화
    async fn on_start(&mut self, rig: &Rig) -> Result<()> { Ok(()) }

    /// 에이전트 루프 전: 컨텍스트를 결정론적으로 사전 수집
    /// (AGENTS.md 읽기, 티켓 참조 fetch, 워크스페이스 컨텍스트 로드)
    async fn pre_hydrate(&self, work: &WorkItem) -> Context { Context::empty() }

    /// 에이전트 완료 후: 결정론적 검증 + 안전망
    /// (lint, test, commit, 에이전트가 잊었으면 PR 생성)
    async fn post_execute(&self, work: &WorkItem) -> Result<()> { Ok(()) }
}
```

기본 미들웨어:
- **ContextHydrator** — AGENTS.md, 워크스페이스 파일 읽어서 프롬프트에 주입
- **ValidationGate** — 에이전트 완료 후 lint + 테스트 실행 (결정론적)
- **SafetyNet** — 에이전트가 잊었으면 커밋 + PR 생성
- **BoundedRetry** — CI 수정 최대 2라운드, 이후 needs-human-review로 표시

### 5.6 Prompt Caching 전략

> 설계 배경: Goose의 Anthropic provider가 자동으로 `cache_control` breakpoint를 설정한다.

**Goose의 자동 캐싱:**
- 매 `Agent::reply()` 호출 시 마지막 user message와 그 이전 message에 `cache_control` 마킹
- Anthropic, Bedrock, Databricks, OpenRouter, LiteLLM provider에서 자동 활성화
- 별도 설정 불필요 (v1.27.2 기준)

**Strategy 패턴이 캐싱을 구조적으로 보장하는 방법:**

| 경로 | 세션 | 캐시 동작 |
|------|------|-----------|
| Operator (ChatMode) | 영속 — 항상 같은 세션 | **full prefix hit**: system prompt + tools + 전체 대화 이력 |
| Worker (TaskMode) | 작업당 새 세션 | system prompt + tools만 hit, 이력 없음 (독립 작업이므로 정상) |

```
Operator (영속 세션):
  Turn 1: [system + tools │ msg1]              → miss, 캐시 쓰기
  Turn 2: [system + tools │ msg1 │ msg2]       → hit (prefix = turn 1)
  Turn 3: [system + tools │ msg1 │ msg2 │ msg3] → hit (prefix = turn 2)

Worker (작업 세션):
  Task A: [system + tools │ task_a]  → system+tools hit (Operator와 공유)
  Task B: [system + tools │ task_b]  → system+tools hit
```

**캐시 레이어:**

```
[항상 캐시됨 — 모든 세션에서 동일]
  ├─ system prompt (recipe instructions + identity)
  ├─ MCP tool definitions (board__* 등)
  └─ extend_system_prompt

[Operator에서만 캐시됨 — 영속 세션]
  └─ conversation history (msg1, msg2, msg3, ...)

[매 턴 변동 — 캐시 안 됨]
  └─ 현재 메시지
```

**구조적 최적화 (선택):**
- `prime()` 내용 중 안정적 부분(board summary template)을 앞에, 변동 부분(최근 완료)을 뒤에 배치
- `pre_hydrate`의 정적 컨텍스트(AGENTS.md)를 system prompt 초반에 배치

### 5.7 보드 도구 (Platform Extension, 내장)

별도 프로세스/바이너리 없이 Goose의 **Platform Extension**으로 내장. `McpClientTrait`을 직접 구현하므로 MCP JSON-RPC 직렬화 오버헤드 제로.

```
board__claim_next     → Board.claim() — 다음 ready 작업 항목 pull
board__create_task    → Board.post() — 하위 작업 생성
board__update_status  → Board.update() — 진행 상황 보고
board__delegate       → Board.post(assigned_to: 동료) — 동료에게 요청
board__broadcast      → Board.broadcast() — 전체에게 알림
board__read_board     → Board.list() — 현재 상태 조회
board__stamp          → Board.stamp() — 동료의 작업 평가 (L3+ 전용)
board__remember       → Memory.write() — 경험 기억 기록 (§ 4.5)
board__recall         → Memory.search() — 경험 기억 검색 (§ 4.5)
board__memory_tree    → Memory.tree() — 메모리 트리 조회 (§ 4.5)
```

등록 방식:
```rust
PlatformExtensionDef {
    name: "board",
    display_name: "Board",
    default_enabled: true,
    unprefixed_tools: false,  // board__claim_next 형태로 노출
    client_factory: |ctx| Box::new(BoardClient::new(ctx, board)),
}
```

이것이 유일한 조율 도구. Goose의 도구 검사 파이프라인을 자동 상속.

### 5.8 Git Worktree + 내장 프록시

> 컨셉 배경: [REFERENCE.md § 5 — Portless](REFERENCE.md#5-portless-vercel-labsportless) (concept only)

#### 5.5.1 Worktree 관리

```rust
pub struct WorktreeHandle {
    path: PathBuf,         // /tmp/og-rigs/{rig-id}/{work-id}/
    branch: String,        // rig/{rig-id}/{work-id}
    ports: Vec<PortBinding>,
}

pub struct PortBinding {
    name: String,          // "web", "api", "storybook"
    internal_port: u16,    // 실제 앱이 사용하는 포트
    proxy_path: String,    // "/api", "/" 등
}
```

#### 5.5.2 내장 프록시 (단일 프로세스)

`opengoose` 시작 시 프록시 서버가 함께 실행 (기본 포트: 1355):

```rust
pub struct EmbeddedProxy {
    routes: Arc<RwLock<HashMap<String, Vec<PortBinding>>>>,  // subdomain → ports
    listen_port: u16,  // 1355
}

impl EmbeddedProxy {
    /// rig가 dev 서버 시작할 때 등록
    pub async fn register(&self, rig_id: &str, ports: Vec<PortBinding>) {
        self.routes.write().await.insert(rig_id.to_string(), ports);
    }

    /// Host 헤더에서 subdomain 추출 → 내부 포트로 라우팅
    async fn proxy_request(&self, req: Request<Body>) -> Response<Body> {
        // "developer.localhost:1355" → subdomain: "developer"
        // path: "/api/users" → PortBinding { name: "api", path: "/api" } 매칭
        // → localhost:{internal_port}/users 로 프록시
    }
}
```

**라우팅 규칙:**
```
developer.localhost:1355/          → 기본 포트 (web)
developer.localhost:1355/api/      → api 포트 (path prefix 매칭)
developer-api.localhost:1355/      → api 포트 (subdomain 방식, 대안)
```

#### 5.5.3 포트 런타임 감지 `[Phase 6 — 최후순위]`

> 참고: VS Code Remote SSH의 포트 자동 포워딩과 동일한 접근.
> VS Code는 Linux에서 `/proc/net/tcp` 폴링, macOS에서 터미널 출력 파싱을 사용한다.
> Phase 6 범위. 단일 rig 환경에서는 프록시 없이 `localhost:{port}` 직접 접근으로 충분.

설정 파일(package.json, docker-compose 등)을 정적으로 파싱하지 않는다. **실제로 열리는 포트를 런타임에 감지:**

```
Goose Agent가 셸 도구로 서버 실행
  │
  ├─ AgentEvent 스트림에서 출력 파싱 (모든 OS)
  │    "ready on http://localhost:3000" → 포트 3000 감지
  │
  └─ [Linux 보완] /proc/net/tcp 폴링 (출력에 URL 안 찍는 서버 대응)
  │    /proc/{pid}/cwd로 worktree 경로 매칭 → rig 식별
  │
  ▼
프록시에 자동 등록
  developer.localhost:1355 → localhost:3000
```

**출력 파싱 (VS Code UrlFinder 방식):**

```rust
// VS Code가 사용하는 정규식 (검증됨):
//   URL 감지: /\b\w{0,20}(?::\/\/)?(?:localhost|127\.0\.0\.1|0\.0\.0\.0|:\d{2,5})[\w\-\.\~:\/\?\#[\]\@!\$&\(\)\*\+\,\;\=]*/gim
//   포트 추출: /(localhost|127\.0\.0\.1|0\.0\.0\.0):(\d{1,5})/
//   Python 전용: /HTTP\son\s(127\.0\.0\.1|0\.0\.0\.0)\sport\s(\d+)/

pub struct PortWatcher {
    baseline: HashSet<u16>,                    // 시작 시 이미 열려있던 포트
    rig_ports: HashMap<RigId, HashSet<u16>>,   // rig별 감지된 포트
}

impl PortWatcher {
    /// AgentEvent::ToolOutput에서 localhost URL 추출
    pub fn scan_output(&mut self, rig_id: &RigId, output: &str) -> Vec<u16> {
        // 10,000자 초과 출력은 무시 (VS Code 동일, 성능 보호)
        // regex로 포트 추출 → 유효 범위 1-65535 확인
        // baseline에 없는 새 포트만 반환
    }
}
```

**`/proc/net/tcp` 폴링 (Linux 보완, VS Code 방식):**

```rust
#[cfg(target_os = "linux")]
impl PortWatcher {
    pub async fn poll_proc_net(&mut self, proxy: &EmbeddedProxy) {
        // 1. /proc/net/tcp 읽기 → st=="0A" (LISTEN) 필터
        // 2. local_address hex → IP:port 변환
        //    IPv4: "0100007F:0BB8" → 127.0.0.1:3000 (2자리씩 역순)
        //    IPv6: 32자리 hex → 8자리씩 처리
        // 3. 소켓→PID: ls -l /proc/[0-9]*/fd/[0-9]* | grep socket:
        //    정규식: /proc/(\d+)/fd/\d+ -> socket:\[(\d+)\]
        // 4. /proc/{pid}/cwd로 worktree 경로 매칭 → rig 식별
        // 5. baseline diff → 새 포트를 프록시에 등록
    }
}

// 폴링 간격 (VS Code 적응형):
//   interval = max(스캔소요시간_이동평균 × 20, 2000ms)
//   처음 3회 스캔은 이동평균에서 제외 (워밍업)
```

**제외 목록:**

| 제외 대상 | 이유 |
|----------|------|
| baseline 포트 (시작 시 스냅샷) | 시스템 서비스 오감지 방지 |
| 포트 9229 | Node.js 디버거 (항상 열림) |
| opengoose 자체 프로세스 | `knownExcludeCmdline` 패턴 매칭 |
| rig당 20개 초과 감지 시 | 경고 + 수동 설정 안내 |

**포트 없는 프로젝트:** CLI 라이브러리, 데이터 처리 등 → 포트 감지 없음, worktree만 사용.

#### 5.5.4 Worktree 생성

OpenGoose 자체에서는 child process를 만들지 않는다:

```rust
impl WorktreeHandle {
    pub fn create(rig_id: &RigId, work: &WorkItem) -> Result<Self> {
        let branch = format!("rig/{}/{}", rig_id, work.id);
        let path = PathBuf::from(format!("/tmp/og-rigs/{}/{}", rig_id, work.id));
        git_worktree_add(&path, &branch)?;
        Ok(Self { path, branch })
    }
}
```

Phase 2에서는 worktree만 생성. 에이전트가 `localhost:{port}`로 직접 접근.
Phase 6에서 프록시 + PortWatcher 추가 시, 감지된 포트가 자동으로 `{rig-id}.localhost:1355`에 매핑.

**에이전트에게 내장 프록시가 중요한 이유 (Phase 6):**
- 여러 rig가 동시에 dev 서버 실행 → 포트 충돌 없음
- 안정적 네임드 URL → 에이전트가 서로의 서비스를 이름으로 참조
- 단일 프록시 포트 (1355) → 방화벽/보안 설정 단순화
- 포트 자동 감지 → 설정 파일 파싱 불필요, 실제 동작 기반

---

## 6. CLI 인터페이스 `[계획]`

### 6.1 대화형 모드

```bash
$ opengoose
> Hello, help me refactor the auth module
# → Operator가 직접 응답 (Board 안 거침, 영속 세션)

> /board
# → 보드 상태 표시 (open/claimed/done 수, rig 상태)

> /task "Implement rate limiting for the API"
# → Board에 태스크 게시 → Worker가 pull → claim → 실행

> /status
# → rig 상태, 신뢰 수준, 현재 작업 표시
```

### 6.2 헤드리스 모드

```bash
# 단일 작업 실행 후 종료
$ opengoose run "Fix the failing CI test in auth.rs"

# 특정 레시피로 실행
$ opengoose run --recipe researcher "Survey rate limiting libraries for Rust"

# 보드 상태 표시
$ opengoose board

# Rig 및 신뢰 수준 목록
$ opengoose rigs
```

### 6.3 작업 제어 명령

```bash
# CI 2라운드 초과 시 추가 2라운드 허용
> /retry 9

# 작업 포기 — worktree 삭제, 작업 항목 → abandoned
> /abandon 9

# stamp — 3차원 모두 지정 필수 (/approve, /reject 없음)
> /stamp 1 q:0.8 r:1.0 h:0.6 branch

# 프로젝트 컨텍스트 전환
> /project ~/dev/backend
```

### 6.4 응답 스트리밍

**Operator 대화:** `operator.chat(input)`이 `Stream<AgentEvent>`를 반환. CLI가 직접 소비하여 토큰 단위 스트리밍.

**Worker 작업:** CLI가 활성 Worker의 스트림을 구독. Worker가 작업을 처리하는 동안 에이전트 출력을 토큰 단위로 스트리밍.

```
Operator: operator.chat(msg) → Stream<AgentEvent> → 터미널 출력 (직접)
Worker:   Board.subscribe(work_id) → tokio::watch::Receiver<BoardEvent>
  → BoardEvent::StreamChunk { work_id, text } → 터미널에 출력
  → BoardEvent::WorkCompleted { work_id } → 요약 표시
```

---

## 7. Beads 알고리즘

### 7.1 ready() — 작업 가능한 것

```rust
/// 열린 블로킹 의존성이 없는 작업 항목 반환
pub fn ready(&self, opts: &ReadyOptions) -> Vec<WorkItem> {
    // 1. 모든 open 항목 조회
    // 2. 열린 의존성에 의해 차단된 항목 필터링
    // 3. 우선순위 정렬 (P0 > P1 > P2)
    // 4. 신뢰 필터 적용 (L1은 L1 적합 작업만 볼 수 있음)
    // 5. 태그 매칭 적용:
    //    - 작업에 태그가 있으면: rig의 recipe 태그와 완전 일치 필요
    //    - 작업에 태그가 없으면: 아무 rig나 claim 가능
    // 6. 세션 친화성 적용 (rig의 현재 세션 항목 우선)
}
```

**태그 매칭 규칙:**
- `work_item.tags = ["researcher"]` → researcher recipe를 가진 rig만 claim
- `work_item.tags = []` → 모든 rig가 claim 가능
- Stripe Toolshed 패턴: 에이전트별 도구 부분집합 + Wasteland 자율성 조합

### 7.2 prime() — 에이전트 컨텍스트 주입

```rust
/// 에이전트 세션 시작을 위한 1-2K 토큰 컨텍스트 요약 생성
pub fn prime(&self, rig_id: &RigId) -> String {
    // 우선순위 분포: P0: 2, P1: 5, P2: 12
    // 블로킹 이슈: #1 blocks #2
    // 준비된 작업: 3개 이용 가능
    // 최근 완료: #3 (2분 전)
    // 에이전트 이력: 5 완료, 1 실패, L2 신뢰
}
```

### 7.3 compact() — 메모리 감쇠

```rust
/// 오래된 완료 항목을 요약하여 저장소 축소
pub fn compact(&self, older_than: Duration) -> Result<()> {
    // 1. 임계값보다 오래된 완료 항목 탐색
    // 2. 각각 AI 요약 생성
    // 3. 전체 내용을 요약으로 대체
    // 4. 보존: id, title, status, relationships, stamps
    // 5. 삭제: 상세 설명, acceptance_criteria, 상세 로그
}
```

---

## 8. 신뢰 모델 (Wasteland)

### 8.1 Stamps

```rust
pub struct Stamp {
    pub target_rig: RigId,       // 누가 평가받는가
    pub work_item: i64,          // 어떤 작업에 대해 (work item id)
    pub dimension: Dimension,    // Quality | Reliability | Helpfulness
    pub score: f32,              // -1.0 ~ +1.0
    pub severity: Severity,      // Leaf(1.0x) | Branch(2.0x) | Root(4.0x)
    pub stamped_by: RigId,       // 누가 평가했는가 (≠ target_rig)
    pub timestamp: DateTime<Utc>,
}

pub enum Dimension {
    Quality,      // 결과물의 품질
    Reliability,  // 약속한 일을 완수했는가
    Helpfulness,  // 팀에 얼마나 도움이 됐는가
}

pub enum Severity {
    Leaf,    // 1.0x — 사소한 작업 (버그 수정 등)
    Branch,  // 2.0x — 중간 규모 작업
    Root,    // 4.0x — 핵심 인프라 작업
}
```

**점수 계산:**
```
weighted_score = Σ(severity_weight × score)
```
Root 작업에서 +1.0 평가 = Leaf 4개분의 가치.

**시간 감쇠 (30일 반감기):**
```rust
fn decayed_score(stamp: &Stamp, now: DateTime<Utc>) -> f32 {
    let days = (now - stamp.timestamp).num_days() as f32;
    let decay = 0.5_f32.powf(days / 30.0);  // 30일 반감기
    stamp.score * stamp.severity.weight() * decay
}
```
과거의 업적으로 영원히 높은 신뢰를 유지할 수 없음.

### 8.2 신뢰 사다리

| 수준 | 가중 점수 | 능력 |
|------|----------|------|
| L1 (Newcomer) | < 3 | 작업 claim만, stamp 불가, 위임 불가 |
| L1.5 (Recognized) | >= 3 | + 하위 작업 생성, 위임 가능 (depth 1) |
| L2 (Contributor) | >= 10 | + 다른 rig에게 stamp 가능, 위임 depth 2 |
| L2.5 (Trusted) | >= 25 | + 최상위 작업 생성, 위임 depth 3 |
| L3 (Veteran) | >= 50 | + 위임 depth 무제한 |

**위임 depth:** 하위 작업을 몇 단계까지 생성할 수 있는가.
```
L1.5: task → subtask (1단계)
L2:   task → subtask → sub-subtask (2단계)
L3:   무제한
```

**제재 (Sanction):**
```rust
if weighted_score < -5.0 {
    rig.mode = RigMode::ReadOnly;  // 읽기만 가능, 쓰기 도구 사용 불가
}
```

### 8.3 졸업앨범 규칙 (Yearbook Rule)

```sql
CHECK (stamped_by != target_rig)
```

보드가 API 레벨에서도 강제. 예외 없음.

### 8.4 초기 부트스트랩

처음에 rig가 하나뿐이면 stamp를 받을 수 없는 문제:

```rust
pub struct RigId(pub String);  // "dh", "researcher-01", "developer-01"
```

사람도 rig이다. 타입 구분 없음. CLI 시작 시 사용자가 L3 trust로 자동 등록될 뿐.

사람도 rig이다. Wasteland의 `rigs` 테이블에서 `rig_type: 'human' | 'ai'`와 동일.

- **사용자 = RigType::Human, 항상 L3** — stamp 가능
- 첫 번째 AI rig는 L1으로 시작
- 사용자가 작업 완료 후 `/stamp`로 평가
- stamp 누적 → 자동 승급

---

## 9. v1 → v0.2 마이그레이션

### 유지하는 것 (컨셉)

- 커스텀 CoW 스토어 + branch/merge (prollytree 크레이트에 문제가 있었음)
- Beads 알고리즘 (ready/prime/compact)
- Stamps + 신뢰 사다리 + 졸업앨범 규칙
- Witness (stuck/zombie 감지)
- 에이전트 통신용 MCP 도구

### 버리는 것

- 21개 중 18개 크레이트
- 커스텀 Engine, GatewayBridge, StreamResponder
- Push 기반 TeamOrchestrator (Chain/FanOut/Router)
- Profile 시스템 (Goose Recipe로 직접 대체)
- 모든 플랫폼 게이트웨이 (Discord, Slack, Telegram, Matrix)
- Federation (나중으로 연기)
- TUI, provider-bridge, secrets, 웹 대시보드
- AgentPool, Deacon, ReviewQueue, DelegationTracker

### 변경되는 것

| 측면 | v1 | v0.2 |
|------|-----|------|
| 메시지 흐름 | Push (오케스트레이터가 할당) | Pull (에이전트가 보드에서 claim) |
| 오케스트레이션 | 전략이 있는 TeamOrchestrator | 보드가 곧 오케스트레이터 |
| 에이전트 수명 | 일회성 (요청마다 생성) | 영속 (Operator: 세션, Worker: pull 루프) |
| 크레이트 수 | 21 | 3 |
| 인터페이스 | Discord + Slack + Telegram + Matrix + Web + CLI | CLI만 |
| 대화 vs 작업 | 구분 없음 (모두 Engine 통과) | 듀얼 패스: Operator(대화) + Worker(Board) |
| Goose 통합 | 깊은 래핑 | 최소한: create + reply + stream |
| 데이터 격리 | 인메모리만 | CoW 브랜치 + git worktree + portless |
| 포트 관리 | 미대응 | Portless 네이밍 |
| 실행 모델 | 에이전트 노드만 | 블루프린트: 결정론적 + 에이전트 노드 |
| 컨텍스트 로딩 | 런타임 발견 | 에이전트 루프 전 사전 수집 |
| CI 검증 | 무제한 | 제한 (최대 2라운드) |

---

## 10. 열린 질문

1. ~~**대화가 보드를 우회해야 하는가?**~~ **해결됨 (§ 2.4).** 대화는 Operator가 직접 처리, Board를 거치지 않음. Board의 가치는 조율이고 대화에는 조율할 것이 없다. Operator는 Board 접근 권한만 보유 (읽기 + 태스크 생성).

2. **Federation 범위?** 전면 연기. v0.2 = 단일 인스턴스 pull 아키텍처.

3. **Dolt 통합은 나중에?** CoW 스토어가 스케일링 한계에 도달하면 Board API 뒤에서 Dolt로 대체 가능.

4. **샌드박스 추상화?** git worktree (로컬)로 시작. 나중에 Docker/Modal/Daytona용 `SandboxBackend` 트레잇 추가.

5. **멀티 rig CLI UX?** 여러 rig가 활성일 때 CLI가 동시 스트림을 어떻게 표시하는가? 옵션: 멀티플렉스 출력, 포커스 모드 (한 번에 하나의 rig), 분할 패널.

6. **Reflection 서브에이전트?** 작업 완료 후 별도 에이전트가 학습을 자동 정리 (Letta 참조). 현재는 에이전트 주도 기록 + pre-compaction flush로 충분하다고 판단. 에이전트가 기록을 자주 잊는 문제가 관찰되면 도입 검토.

7. **QMD 사이드카 시점?** Phase 2는 BM25(SQLite FTS5). 메모리 파일이 수천 개 이상 쌓이거나 시맨틱 검색이 필요한 시점에서 QMD 도입. Node.js 의존성 vs Rust 포팅 결정도 그때.

8. **Per-Rig 메모리의 이식성?** Rig를 삭제하면 per-rig 메모리도 삭제되는가? 보존이 필요하면 per-project로 승격하는 메커니즘이 필요.

9. **Pre-compaction flush의 Goose 통합 방법?** Goose의 자동 압축 훅이 공개 API로 노출되어 있는지 확인 필요. 없다면 토큰 사용량 모니터링으로 근사 (§ 4.5.3).

10. **Git 코드 머지 전략 (Refinery)?** 여러 rig가 동시에 코드 작업을 하면 git merge가 필요. Phase 2에서는 순차 머지 (먼저 끝난 rig가 먼저 main에 머지). 순차 머지에서 git 충돌이 자주 발생하면 Gas Town의 Refinery 패턴 (배치 리베이스 → 테스트 → 실패 시 이등분 격리) 도입 검토. CoW store 머지(보드 데이터)와 git merge(코드)는 별개 — 보드는 Beads 4규칙, git은 git 자체 머지.

11. **보드 계층 구조 (상위 프로젝트)?** Phase 1a는 단일 보드. 추후 GitHub Organization → Repository 구조처럼 보드 계층을 도입 검토. 상위 보드(umbrella, project 디렉토리 없음)가 하위 보드들을 관리하고, 크로스 프로젝트 태스크를 상위 보드에서 생성하여 하위 보드에 하위 작업으로 분배. Rig의 활동 범위(scope)가 Trust와 별도의 축이 됨: 상위 보드 rig는 모든 하위 보드 접근 가능, 하위 보드 rig는 자기 보드만. WorkItem.parent가 크로스 보드를 가리킬 수 있으려면 `Option<WorkItemRef>` (board_id + item_id) 필요.
