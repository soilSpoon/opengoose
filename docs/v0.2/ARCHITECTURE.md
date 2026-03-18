# OpenGoose v0.2 아키텍처

> **작성일:** 2026-03-18
> **목표:** Goose-native pull 아키텍처 + Wasteland 수준 에이전트 자율성
> **원칙:** Goose가 에이전트 작업을 한다. OpenGoose는 조율만 한다.
> **인터페이스:** CLI만. Discord/Slack/Telegram/Matrix 없음.
> **구현 상태:** `[구현]` = 코드 존재, `[계획]` = 설계만

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

| 출처 | 변환 |
|------|------|
| CLI 사용자 메시지 | 작업 항목 (type: conversation) |
| `opengoose task "..."` | 작업 항목 (type: task) |
| 에이전트가 하위 작업 생성 | 작업 항목 (type: subtask, parent: 원본) |
| 에이전트가 동료에게 위임 | 작업 항목 (type: delegation, assigned_to: 동료) |
| Cron 스케줄 실행 | 작업 항목 (type: scheduled) |

특별한 메시지 라우팅 없음. 팀 오케스트레이션 로직 없음. 보드가 곧 오케스트레이터.

```rust
pub enum WorkType {
    Conversation,  // 대화 (프로젝트 있으면 코드 접근 가능)
    Task,          // 코드 작업 (프로젝트 필수)
}
```

### 2.3 프로젝트 컨텍스트

Rig와 프로젝트는 독립적인 축이다. Rig는 프로젝트에 대해 모른다 — 작업 항목이 프로젝트를 알고 있고, rig는 claim한 작업의 컨텍스트를 받아서 사용할 뿐.

```rust
pub struct WorkItem {
    pub hash_id: HashId,
    pub title: String,
    pub status: Status,
    pub work_type: WorkType,
    pub project: Option<ProjectRef>,  // 작업의 속성이지, rig의 속성이 아님
    // ...
}

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
  ├─ bd-a1b2 claim (project: myapp)   → working_dir = ~/dev/myapp
  ├─ bd-c3d4 claim (project: backend) → working_dir = ~/dev/backend
  └─ bd-e5f6 claim (project: None)    → worktree 없음, 범용
```

### 2.4 대화도 작업 항목이다

CLI에서 대화하면 conversation 타입 작업 항목 스트림이 생성된다:

```
사용자 메시지 #1 → 작업 항목 (session: cli-1, seq: 1) → Rig A가 claim
사용자 메시지 #2 → 작업 항목 (session: cli-1, seq: 2) → Rig A가 claim (세션 친화성)
사용자 메시지 #3 → 작업 항목 (session: cli-1, seq: 3) → Rig A가 claim
```

세션 친화성: 세션의 첫 메시지를 claim한 rig가 후속 메시지에 대해 우선권을 가진다. **해당 rig가 현재 작업 중이면** 다른 rig가 claim할 수 있다 (Wasteland 패턴: 디스패처 없이 claim 상태만 체크).

**제약 사항:** 다른 rig가 세션을 인계받으면 Goose 대화 이력이 단절된다. Goose의 `SessionManager`는 에이전트 인스턴스에 바인딩되므로, 인계 rig는 새 세션으로 시작하되 `prime()` 컨텍스트에 이전 대화 요약을 포함하는 방식으로 연속성을 근사한다. 완벽한 이력 공유는 Goose의 세션 fork/export API로 가능하지만, 구현 복잡도가 높아 Phase 2로 연기.

Goose의 `SessionManager`가 대화 이력을 내부적으로 관리. 보드는 작업 항목 수명주기만 추적.

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
├── Cargo.toml                    # [구현] 워크스페이스
├── crates/
│   ├── opengoose/                # 바이너리 — CLI (대화형 + 헤드리스)
│   │   └── src/
│   │       ├── main.rs           # [구현] 진입점 (스텁)
│   │       ├── cli.rs            # [계획] 대화형 REPL (사용자 입력 → 보드)
│   │       ├── run.rs            # [계획] 헤드리스 모드 (레시피 → 보드 → 실행 → 종료)
│   │       └── status.rs         # [계획] 보드 상태 표시
│   │
│   ├── opengoose-board/          # [구현] Wanted Board + Beads + 데이터
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── board.rs          # WantedBoard: post/claim/submit/merge
│   │       ├── work_item.rs      # WorkItem, HashId, Status, Priority
│   │       ├── store.rs          # 커스텀 CoW BTreeMap 스토어 (Dolt 영감)
│   │       ├── branch.rs         # 에이전트별 브랜치 격리 + 3-way merge
│   │       ├── merge.rs          # 셀 레벨 충돌 해결
│   │       ├── beads.rs          # ready() / prime() / compact()
│   │       ├── stamps.rs         # Stamps + Trust Ladder + Yearbook Rule
│   │       └── relations.rs      # 의존성 그래프 (blocks, depends_on)
│   │
│   └── opengoose-rig/            # [구현] Agent Rig (영속 pull 루프)
│       └── src/
│           ├── lib.rs
│           ├── rig.rs            # Rig: 정체성, 루프, 수명주기
│           ├── executor.rs       # Goose Agent::reply() 래퍼 (최소한)
│           ├── worktree.rs       # Git worktree 관리
│           ├── portless.rs       # Portless 스타일 네임드 URL 할당
│           ├── witness.rs        # Stuck/zombie 감지
│           ├── middleware.rs     # Before/after 훅 (컨텍스트 사전 수집, 안전망)
│           └── mcp_tools.rs      # MCP 서버: 에이전트용 보드 도구
```

### 3.1 의존성 그래프

```
opengoose-board           (OpenGoose 의존성 없음, serde/chrono/uuid/tokio/sha2만)
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
│  Work items (이 프로젝트의 작업들)               │
│  Relations (작업 간 의존성)                      │
│  CoW branches + commit log                       │
│  프로젝트 컨텍스트 (AGENTS.md 등)                │
└──────────────────────────────────────────────────┘
```

| 데이터 | 스코프 | 이유 |
|--------|--------|------|
| Stamps, Trust | **Global** | 연구자의 능력은 어디서 일했든 동일 |
| Rig 설정 (recipe, auto_start) | **Global** | 프로젝트 무관 |
| Rig 상태 + 이력 | **Per-Rig** | rig 자체에 누적, 프로젝트 단위가 아님 |
| Work items, Relations | **Per-Project** | 특정 프로젝트의 작업 |
| CoW branches, Commit log | **Per-Project** | 브랜치 격리는 프로젝트 데이터에 대해 |
| 대화 이력 | **Goose Session** | Goose가 관리, 우리가 신경 안 씀 |

**Per-Rig 데이터** (Gas Town의 Agent Bead, Wasteland의 completions에서 참조):

```rust
pub struct RigState {
    // 현재 상태
    pub status: RigStatus,           // Idle, Working, Stuck, Zombie
    pub current_work: Option<HashId>, // 지금 뭘 하고 있는지
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
    pub work_item: HashId,
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
├── global.db                      # Global: stamps, trust, rig 이력
│
├── projects/
│   ├── myapp/                     # Per-Project: ~/dev/myapp
│   │   ├── board.db               # work items, relations
│   │   └── cow_store/             # CoW 스냅샷, WAL, branches
│   │
│   └── backend/                   # Per-Project: ~/dev/backend
│       ├── board.db
│       └── cow_store/
│
└── unscoped/                      # 프로젝트 없는 작업
    └── board.db
```

**BoardManager:**

```rust
pub struct BoardManager {
    global: GlobalStore,                          // stamps, trust, rig 정보
    projects: HashMap<String, ProjectBoard>,       // 프로젝트별 보드
    unscoped: ProjectBoard,                        // 프로젝트 없는 보드
}

impl BoardManager {
    fn board_for(&self, project: Option<&str>) -> &ProjectBoard {
        match project {
            Some(name) => &self.projects[name],
            None => &self.unscoped,
        }
    }

    // Trust는 전역 (모든 프로젝트의 stamp 합산)
    fn trust_level(&self, rig_id: &RigId) -> TrustLevel {
        self.global.compute_trust(rig_id)
    }

    // ready()는 특정 프로젝트 또는 전체에서 조회
    fn ready(&self, rig_id: &RigId, project: Option<&str>) -> Vec<WorkItem> {
        match project {
            Some(name) => self.projects[name].store.ready(rig_id),
            None => self.all_boards().flat_map(|b| b.store.ready(rig_id)).collect(),
        }
    }
}
```

### 4.2 스토리지: 커스텀 CoW 스토어 (prollytree 크레이트 아님)

prollytree Rust 크레이트 (v0.3.2-beta)는 v1 개발 중 문제가 있었다. v0.2는 Dolt의 prolly tree 컨셉에서 영감받은 커스텀 구현을 사용한다:

**핵심: 콘텐츠 주소 지정이 가능한 Copy-on-Write BTreeMap**

```rust
pub struct CowStore {
    data: Arc<BTreeMap<Key, Value>>,  // O(1) 브랜칭을 위한 Arc
    root_hash: OnceCell<RootHash>,    // 캐시됨, 변이 시 무효화
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

> SQLite 의존성은 아직 Cargo.toml에 없다. 아래는 목표 아키텍처.

**원칙: 재시작해도 상태 유지** (Beads/Wasteland 패턴: append-only 장부, 즉시 롤백)

```
┌─ 인메모리 (빠름) ─────────────────────┐
│  CowStore (BTreeMap, Arc CoW)         │
│  - 모든 작업 항목, 관계               │
│  - 에이전트별 브랜치                  │
│  - 루트 해시 캐싱                     │
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

```
Rig 생성
  → board.branch("rig-researcher")     // main의 Arc clone, O(1)

Rig가 작업 claim
  → 브랜치에 쓰기                      // CoW: 첫 쓰기 시 BTreeMap clone 발생
  → board.commit("claimed bd-a1b2")    // 루트 해시 스냅샷

Rig가 작업 완료
  → board.merge(branch, main)          // 3-way: base(스냅샷) vs branch vs main
  → 충돌? → 셀 레벨 해결
  → 커밋 로그 항목 (해시 체인)

Rig 실패
  → board.drop(branch)                 // main은 영향 없음
```

### 4.5 충돌 해결 (Dolt 영감, 셀 레벨)

```rust
pub enum FieldStrategy {
    SourceWins,           // 에이전트 버전 우선
    DestWins,             // main 버전 우선
    HigherStatusWins,     // completed > failed > in_progress > pending
    LatestTimestamp,       // 더 새로운 updated_at 우선
    Immutable,            // base 값 유지 (hash_id, created_at)
    Union,                // 배열 병합 (labels, acceptance_criteria)
}
```

필드별 3-way merge:
- 한쪽만 변경 → 그 변경 채택 (자동 머지)
- 양쪽이 같은 필드 변경 → 필드 전략 적용
- 같은 항목의 다른 필드 변경 → 둘 다 적용 (충돌 아님)

---

## 5. Rig 아키텍처

### 5.1 Rig = 영속 에이전트 정체성 + Pull 루프

```rust
pub struct Rig {
    pub id: RigId,                    // 안정적 정체성 (영속)
    pub recipe: String,               // Goose 레시피 이름 (v1의 "profile" 대체)
    pub trust_level: TrustLevel,      // L1..L3 (stamps에서 파생)
    pub session_id: Option<String>,   // 현재 Goose 세션
    agent: Option<Agent>,             // Goose Agent (첫 claim 시 생성)
    board: Arc<Board>,                // 보드에 대한 공유 참조
    worktree: Option<WorktreeHandle>, // Git worktree (코드 작업 시)
    middleware: Vec<Box<dyn Middleware>>, // Pre/post 훅
    cancel: CancellationToken,
}

impl Rig {
    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                // 보드에서 작업 pull
                work = self.board.wait_for_claimable(&self.id) => {
                    self.execute(work).await;
                }
                _ = self.cancel.cancelled() => break,
            }
        }
    }

    async fn execute(&mut self, work: WorkItem) {
        // 1. 격리를 위한 브랜치 생성
        let branch = self.board.branch(&self.id);

        // 2. 컨텍스트 사전 수집 (결정론적 — LLM 호출 없음)
        let context = self.middleware.pre_hydrate(&work).await;

        // 3. Prime (Beads 요약)
        let prime = self.board.prime(&self.id);

        // 4. Goose 에이전트 생성/재사용
        let agent = self.ensure_agent().await;

        // 5. 선택적: 코드 작업용 git worktree
        if work.needs_code_isolation() {
            self.worktree = Some(WorktreeHandle::create(&self.id, &work)?);
        }

        // 6. Goose로 실행 (에이전트 루프 — 이것을 재구현하지 않는다)
        let message = work.to_message_with_context(&context, &prime);
        let stream = agent.reply(message, session_config, Some(self.cancel.clone()));

        // 7. 결과 스트리밍
        self.process_stream(stream, &work).await;

        // 8. 실행 후 훅 (결정론적 — lint, test, PR)
        self.middleware.post_execute(&work).await;

        // 9. 브랜치 머지
        self.board.merge(branch, "main");

        // 10. worktree 정리
        if let Some(wt) = self.worktree.take() {
            wt.cleanup();
        }
    }
}
```

### 5.2 Goose 통합 (최소한)

Rig가 Goose와 하는 것은 정확히 세 가지:

1. **Agent 생성** — Recipe로부터 (v1의 profile 대체)
2. **`agent.reply()` 호출** — 작업 항목을 사용자 메시지로
3. **`AgentEvent` 스트림 소비** — 결과와 liveness 확인

나머지 전부 (MCP 도구 디스패치, 컨텍스트 관리, 에러 복구, 프로바이더 추상화)는 Goose의 몫.

### 5.3 미들웨어 훅

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

### 5.4 보드 도구 (Platform Extension, 내장)

별도 프로세스/바이너리 없이 Goose의 **Platform Extension**으로 내장. `McpClientTrait`을 직접 구현하므로 MCP JSON-RPC 직렬화 오버헤드 제로.

```
board__claim_next     → Board.claim() — 다음 ready 작업 항목 pull
board__create_task    → Board.post() — 하위 작업 생성
board__update_status  → Board.update() — 진행 상황 보고
board__delegate       → Board.post(assigned_to: 동료) — 동료에게 요청
board__broadcast      → Board.broadcast() — 전체에게 알림
board__read_board     → Board.list() — 현재 상태 조회
board__stamp          → Board.stamp() — 동료의 작업 평가 (L3+ 전용)
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

### 5.5 Git Worktree + 내장 프록시

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

#### 5.5.3 포트 자동 감지

프로젝트 설정 파일에서 포트 정보 추출:

```rust
pub struct ProjectPorts {
    pub detected: Vec<PortConfig>,   // 자동 감지
    pub manual: Vec<PortConfig>,     // 사용자 명시 (~/.opengoose/projects.yaml)
}

pub struct PortConfig {
    pub name: String,      // "web", "api", "storybook"
    pub port: u16,         // 3000, 8080, 6006
    pub command: String,   // "npm run dev", "cargo run"
    pub path_prefix: String, // "/", "/api"
}
```

**감지 소스:**

| 파일 | 감지 패턴 |
|------|----------|
| `package.json` | `"dev": "next dev -p 3000"`, `"start": "node server.js"` |
| `Procfile` | `web: node server.js` |
| `docker-compose.yml` | `ports: ["3000:3000", "8080:8080"]` |
| `.env` | `PORT=3000`, `API_PORT=8080` |
| `Cargo.toml` | `[package.metadata.opengoose] port = 8080` |
| `opengoose.yaml` | 명시적 설정 (프로젝트 루트) |

**opengoose.yaml 예시:**
```yaml
ports:
  - name: web
    port: 3000
    command: "npm run dev"
    path: "/"
  - name: api
    port: 8080
    command: "cargo run"
    path: "/api"
  - name: storybook
    port: 6006
    command: "npm run storybook"
    path: "/storybook"
```

**포트 없는 프로젝트:** CLI 라이브러리, 데이터 처리 등 → 프록시 등록 안 함, worktree만 사용.

#### 5.5.4 환경변수 주입

```rust
impl WorktreeHandle {
    pub fn create(rig_id: &RigId, work: &WorkItem, proxy: &EmbeddedProxy) -> Result<Self> {
        // 1. Git worktree 생성
        let branch = format!("rig/{}/{}", rig_id, work.hash_id);
        let path = PathBuf::from(format!("/tmp/og-rigs/{}/{}", rig_id, work.hash_id));
        git_worktree_add(&path, &branch)?;

        // 2. 프로젝트 포트 감지
        let ports = detect_project_ports(&path)?;

        // 3. 프록시 등록
        proxy.register(rig_id, ports.clone()).await;

        // 4. 환경변수 주입
        let base_url = format!("http://{}.localhost:{}", rig_id, proxy.listen_port);
        std::env::set_var("OPENGOOSE_URL", &base_url);
        
        // 각 포트별 URL도 주입
        for p in &ports {
            let var_name = format!("OPENGOOSE_{}_URL", p.name.to_uppercase());
            let url = format!("{}{}", base_url, p.path_prefix);
            std::env::set_var(&var_name, &url);
        }

        Ok(Self { path, branch, ports })
    }
}
```

**주입되는 환경변수 예시:**
```bash
OPENGOOSE_URL=http://developer.localhost:1355
OPENGOOSE_WEB_URL=http://developer.localhost:1355/
OPENGOOSE_API_URL=http://developer.localhost:1355/api
OPENGOOSE_STORYBOOK_URL=http://developer.localhost:1355/storybook
```

**에이전트에게 내장 프록시가 중요한 이유:**
- 여러 rig가 동시에 dev 서버 실행 → 포트 충돌 없음
- 안정적 네임드 URL → 에이전트가 서로의 서비스를 이름으로 참조
- 단일 프록시 포트 (1355) → 방화벽/보안 설정 단순화
- 환경변수 자동 주입 → 에이전트가 자기 URL을 프로그래밍적으로 발견

---

## 6. CLI 인터페이스 `[계획]`

### 6.1 대화형 모드

```bash
$ opengoose
> Hello, help me refactor the auth module
# → 작업 항목 게시 → rig가 claim → 응답 스트리밍

> /board
# → 보드 상태 표시 (open/claimed/done 수, rig 상태)

> /task "Implement rate limiting for the API"
# → task 작업 항목 게시 → rig(들)이 하위 작업 claim

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

### 6.3 응답 스트리밍

CLI가 활성 세션의 보드 완료를 구독. Rig가 작업을 처리하는 동안 에이전트 출력을 토큰 단위로 스트리밍.

```
Board.subscribe(session_id) → tokio::watch::Receiver<BoardEvent>
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
    // 블로킹 이슈: bd-a1b2 blocks bd-c3d4
    // 준비된 작업: 3개 이용 가능
    // 최근 완료: bd-f5e6 (2분 전)
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
    // 4. 보존: hash_id, title, status, relationships, stamps
    // 5. 삭제: 상세 설명, acceptance_criteria, 상세 로그
}
```

---

## 8. 신뢰 모델 (Wasteland)

### 8.1 Stamps

```rust
pub struct Stamp {
    pub target_rig: RigId,       // 누가 평가받는가
    pub work_item: HashId,       // 어떤 작업에 대해
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
pub struct HumanUser {
    pub id: UserId,
    pub trust_level: TrustLevel,  // 항상 L3 (Veteran)
}
```

- **사용자(human) = 암묵적 L3 rig** — stamp 가능
- 첫 번째 AI rig는 L1으로 시작
- 사용자가 작업 완료 후 `/approve` 또는 `/stamp`로 평가
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
| 에이전트 수명 | 일회성 (요청마다 생성) | 영속 (rig 루프) |
| 크레이트 수 | 21 | 3 |
| 인터페이스 | Discord + Slack + Telegram + Matrix + Web + CLI | CLI만 |
| Goose 통합 | 깊은 래핑 | 최소한: create + reply + stream |
| 데이터 격리 | 인메모리만 | CoW 브랜치 + git worktree + portless |
| 포트 관리 | 미대응 | Portless 네이밍 |
| 실행 모델 | 에이전트 노드만 | 블루프린트: 결정론적 + 에이전트 노드 |
| 컨텍스트 로딩 | 런타임 발견 | 에이전트 루프 전 사전 수집 |
| CI 검증 | 무제한 | 제한 (최대 2라운드) |

---

## 10. 열린 질문

1. **대화가 보드를 우회해야 하는가?** 기울기: 모든 것을 보드를 통해. 아키텍처가 단순해지고 감사 가능해진다.

2. **Federation 범위?** 전면 연기. v0.2 = 단일 인스턴스 pull 아키텍처.

3. **Dolt 통합은 나중에?** CoW 스토어가 스케일링 한계에 도달하면 Board API 뒤에서 Dolt로 대체 가능.

4. **샌드박스 추상화?** git worktree (로컬)로 시작. 나중에 Docker/Modal/Daytona용 `SandboxBackend` 트레잇 추가.

5. **멀티 rig CLI UX?** 여러 rig가 활성일 때 CLI가 동시 스트림을 어떻게 표시하는가? 옵션: 멀티플렉스 출력, 포커스 모드 (한 번에 하나의 rig), 분할 패널.
