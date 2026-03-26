# OpenGoose v0.2 아키텍처

> **최초 작성:** 2026-03-18
> **마지막 업데이트:** 2026-03-25
> **목표:** Goose-native pull 아키텍처 + Wasteland 수준 에이전트 자율성
> **원칙:** Goose가 에이전트 작업을 한다. OpenGoose는 조율만 한다.

---

## 1. 왜 v0.2인가

v1은 21개 크레이트로 불어났고, Goose가 이미 제공하는 것들(세션, 퍼미션, 컨텍스트 관리)을 재구현했다. 아키텍처는 push 기반이었고 그 위에 pull 컨셉을 덧씌운 형태였다. prollytree Rust 크레이트에 문제가 있어서 커스텀 인메모리 구현으로 대체했는데, 이는 원래 영감을 준 Dolt 컨셉에서 점점 멀어졌다.

v0.2의 설계 제약:

1. **Goose-native** — `Agent::reply()`가 유일한 LLM 인터페이스. 래퍼 없음, 재구현 없음.
2. **Pull-only** — 모든 작업이 Wanted Board를 통과. 오케스트레이터 push 없음.
3. **6개 크레이트** — `opengoose`, `opengoose-board`, `opengoose-rig`, `opengoose-skills`, `opengoose-evolver`, `opengoose-sandbox` (실험적).
4. **CLI-first** — TUI 대화형 + 헤드리스 `run` + 웹 대시보드. 플랫폼 게이트웨이 없음.

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
Worker (루핑) → Board.claim() → Goose.reply() → Board.submit()
```

CLI는 어떤 에이전트가 메시지를 처리할지 모른다. 작업을 게시할 뿐.

### 2.2 모든 것은 작업 항목이다

| 출처 (예시) | 보드에 들어가는 것 |
|-------------|-------------------|
| `opengoose board create "..."` | WorkItem |
| `opengoose run "..."` | WorkItem (헤드리스) |
| 에이전트가 하위 작업 생성 | WorkItem (parent: 미구현) |

**WorkType enum은 없다.** 모든 출처가 동일한 `WorkItem` struct로 변환된다. worktree 생성 여부, 대화인지 코드 작업인지는 rig가 실행 시점에 판단한다.

```rust
pub struct WorkItem {
    pub id: i64,                      // AUTO INCREMENT
    pub title: String,
    pub description: String,
    pub created_by: RigId,
    pub created_at: DateTime<Utc>,
    pub status: Status,               // 더 진행된 쪽이 이김
    pub priority: Priority,           // 더 긴급한 쪽이 이김
    pub tags: Vec<String>,            // 양쪽 합집합
    pub claimed_by: Option<RigId>,
    pub updated_at: DateTime<Utc>,
}
```

**아직 구현되지 않은 필드 (Phase 후반):** `project`, `parent`, `session_id`, `seq`, `assigned_to`, `notes`, `result`.

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

### 2.3 듀얼 패스 아키텍처 — Operator와 Worker

대화와 작업은 다른 경로를 탄다.

```
대화 (hot path):
  User → Operator.chat(msg) → Agent.reply(영속 세션) → stream 응답
                                  Board 안 거침. WorkItem 안 만듦.

작업 (cold path):
  User → Board.post(task) → Worker.pull() → claim → Agent.reply(작업 세션) → submit
```

왜 대화를 Board에서 분리하는가:
- 대화에는 조율할 것이 없다 (1:1, 경쟁 불필요)
- 영속 세션을 유지해야 prompt caching이 보장된다

Operator는 Board에 **접근 권한은 있다** (읽기, 태스크 생성). Board를 **통과하지 않을 뿐이다.**

### 2.4 블루프린트 패턴

복잡한 작업은 결정론적 노드 + 에이전트 노드를 교차 사용:

```
사용자가 작업 게시
  → [결정론적] 컨텍스트 사전 수집 (AGENTS.md, 스킬 카탈로그, Board prime)
  → [결정론적] git worktree 생성
  → [에이전트] 구현 (Goose Agent::reply)
  → [결정론적] cargo check / cargo test / npm test
  → [에이전트] 실패 수정 (최대 2라운드)
  → [결정론적] submit 또는 stuck 마킹
```

결정론적 노드는 토큰을 절약하고 예측 가능하다. 에이전트 노드는 열린 추론을 담당.

---

## 3. 크레이트 구조

```
opengoose/
├── Cargo.toml                           # 워크스페이스
├── crates/
│   ├── opengoose/                       # 바이너리 — CLI, TUI, Web
│   │   └── src/
│   │       ├── main.rs                  # 진입점 (CLI parse → dispatch)
│   │       ├── cli/
│   │       │   ├── mod.rs               # Cli struct, Commands enum, 로깅 설정
│   │       │   ├── commands.rs          # dispatch — TUI/headless/subcommand 분기
│   │       │   └── setup.rs             # home_dir, db_url 헬퍼
│   │       ├── runtime.rs               # Board + Worker + Web + Evolver 와이어링
│   │       ├── headless.rs              # `opengoose run "..."` — 단일 작업
│   │       ├── tui/
│   │       │   ├── app/                 # 앱 상태 (chat, board, logs 탭)
│   │       │   ├── ui/                  # ratatui 렌더링
│   │       │   ├── event/               # 키보드 이벤트, 명령 디스패치
│   │       │   ├── tui_layer.rs         # tracing TuiLayer (로그 → 채널)
│   │       │   └── log_entry.rs         # 로그 파일 회전
│   │       ├── web/
│   │       │   ├── api/                 # REST: board, rigs, skills
│   │       │   ├── pages.rs             # 대시보드 HTML
│   │       │   └── sse.rs               # Server-Sent Events
│   │       ├── skills/                  # Skills CLI 서브커맨드 핸들러
│   │       ├── commands/                # board, rigs CLI 서브커맨드 핸들러
│   │       └── logs.rs                  # 대화 로그 CLI 핸들러
│   │
│   ├── opengoose-board/                 # Wanted Board + Beads + CoW Store
│   │   └── src/
│   │       ├── board.rs                 # Board struct (SQLite + CowStore + Notify)
│   │       ├── work_item.rs             # WorkItem, Status, Priority, RigId, BoardError
│   │       ├── work_items/              # CRUD + 상태 전이 + 쿼리
│   │       ├── store/
│   │       │   ├── mod.rs               # CowStore (Arc<BTreeMap>, Commit)
│   │       │   ├── merge.rs             # 3-way merge (LWW, max-register, G-set)
│   │       │   └── persist.rs           # CowStore ↔ SQLite 동기화
│   │       ├── branch.rs                # Branch (스냅샷 기반 격리)
│   │       ├── merge.rs                 # Mergeable trait, MergeStrategy enum
│   │       ├── beads.rs                 # ready() / prime() / compact()
│   │       ├── stamps.rs                # Severity, TrustLevel, DimensionScores
│   │       ├── stamp_ops.rs             # add_stamp, trust_level, stamp 조회
│   │       ├── rigs.rs                  # rig 등록/삭제/조회
│   │       ├── relations.rs             # RelationGraph (블로킹 의존성)
│   │       └── entity/                  # SeaORM 엔티티 (work_item, stamp, rig, relation, commit_log)
│   │
│   ├── opengoose-rig/                   # Agent Rig (Strategy 패턴)
│   │   └── src/
│   │       ├── rig/
│   │       │   ├── mod.rs               # Rig<M: WorkMode>, Operator/Worker/Evolver 타입 별칭
│   │       │   ├── operator.rs          # Operator: chat, chat_streaming
│   │       │   └── worker.rs            # Worker: run (pull loop), bounded retry
│   │       ├── work_mode.rs             # WorkMode trait, ChatMode, TaskMode, EvolveMode
│   │       ├── pipeline.rs              # Middleware trait, ContextHydrator, ValidationGate
│   │       ├── middleware.rs             # pre_hydrate (AGENTS.md 주입), post_execute (lint/test)
│   │       ├── mcp_tools/               # BoardClient (McpClientTrait — Board 도구)
│   │       ├── worktree.rs              # WorktreeGuard (RAII), sweep_orphaned_worktrees
│   │       └── conversation_log/        # 세션별 대화 로그 (파일 기반)
│   │
│   ├── opengoose-skills/                # 스킬 로딩, 진화, 관리
│   │   └── src/
│   │       ├── catalog.rs               # 스킬 카탈로그 (로드된 스킬 목록)
│   │       ├── loader.rs                # 파일시스템에서 스킬 로드
│   │       ├── metadata.rs              # 스킬 프론트매터 파싱
│   │       ├── lifecycle.rs             # 스킬 수명주기 (active, deprecated 등)
│   │       ├── source.rs                # 스킬 소스 (local, bundled)
│   │       ├── manage/                  # add, remove, list, update, promote, discover, lock
│   │       └── evolution/               # stamp 기반 스킬 자동 생성/개선
│   │           ├── parser.rs            # LLM 응답 파싱
│   │           ├── prompts.rs           # Evolver 프롬프트 빌더
│   │           ├── validator.rs         # 생성된 스킬 검증
│   │           └── writer/              # 스킬 파일 쓰기 (effectiveness, refine)
│   │
│   ├── opengoose-evolver/               # Evolver — stamp 기반 스킬 자동 진화
│   │   └── src/
│   │       ├── lib.rs                   # AgentCaller trait, run() 진입점
│   │       ├── loop_driver.rs           # stamp_notify 대기 + lazy Agent init
│   │       ├── pipeline.rs              # stamp → LLM 분석 → 스킬 생성
│   │       └── sweep.rs                 # 주기적 미처리 stamp 스캔
│   │
│   └── opengoose-sandbox/               # 실험적 — microVM 샌드박스 (macOS HVF)
│       └── src/
│           ├── hypervisor/              # HVF (Apple Hypervisor.framework)
│           ├── boot.rs                  # VM 부팅 시퀀스
│           ├── machine.rs              # VM 머신 설정
│           ├── pool.rs                 # VM 풀 관리
│           ├── snapshot.rs             # CoW 스냅샷
│           ├── vm.rs                   # VM 라이프사이클
│           ├── uart.rs                # 시리얼 콘솔
│           ├── virtio.rs             # VirtIO 장치
│           └── initramfs.rs          # initramfs 빌더
```

### 3.1 의존성 그래프

```
opengoose-board           (OpenGoose 의존성 없음. sea-orm, chrono, serde, tokio)
       ↑
opengoose-rig             (의존: board, goose)
       ↑
opengoose-evolver         (의존: board, rig, skills, goose)
       ↑
opengoose                 (의존: board, rig, skills, evolver — 바이너리)

opengoose-skills          (독립. board, rig, goose 의존 없음)
opengoose-sandbox         (독립. macOS 전용, HVF 의존)
```

### 3.2 각 크레이트가 하지 않는 것

| 크레이트 | 하지 않는 것 |
|----------|-------------|
| **board** | LLM 호출, 세션 관리, 도구 실행, 플랫폼 인식 |
| **rig** | 메시지 라우팅, 플랫폼 관리, 데이터 저장, 텍스트 프로토콜 파싱 |
| **skills** | LLM 호출, Board 접근, Goose 의존 |
| **sandbox** | LLM 호출, Board 접근, 네트워크, 디스크 영속성 |
| **opengoose** | 비즈니스 로직 포함 (CLI + TUI + Web + 와이어링만) |
| **evolver** | Board CRUD, 세션 관리, CLI/TUI, 직접 스킬 파일 I/O (opengoose-skills에 위임) |
| **sandbox** | LLM 호출, Board 접근, 네트워크, 플랫폼 추상화 (macOS HVF 전용) |

---

## 4. 데이터 레이어

### 4.1 Board — SQLite + CowStore 듀얼 구조

```rust
pub struct Board {
    db: DatabaseConnection,        // SeaORM SQLite — 영속성, CRUD, 쿼리
    notify: Arc<Notify>,           // Worker에게 새 작업 알림
    stamp_notify: Arc<Notify>,     // Evolver에게 새 stamp 알림
    store: Mutex<CowStore>,        // 인메모리 — 브랜치/머지 연산
}
```

**Board는 두 가지 역할을 동시에 수행:**
- **SQLite** — 영속성, CRUD, stamp/rig/relation 관리, 쿼리
- **CowStore** — 인메모리 BTreeMap, O(1) 브랜칭, 3-way merge

Board 시작 시 `CowStore::restore(&db)`로 SQLite에서 WorkItem을 로드. `merge()` 후 `persist(&db)`로 변경 사항을 SQLite에 기록.

### 4.2 CowStore — 콘텐츠 주소 지정 Copy-on-Write BTreeMap

```rust
pub struct CowStore {
    main: Arc<BTreeMap<i64, WorkItem>>,  // key: 작업 ID
    commits: Vec<Commit>,                // append-only 커밋 로그
    next_commit_id: u64,
}

pub struct Commit {
    pub id: CommitId,
    pub parent: Option<CommitId>,
    pub root_hash: [u8; 32],            // SHA-256
    pub branch: RigId,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}
```

유지하는 Dolt 컨셉:
- **O(1) 브랜칭** — `Arc::clone(&self.main)`, 첫 쓰기 시 `Arc::make_mut` CoW
- **3-way merge** — Base(분기 시점) vs source(브랜치) vs dest(main), 셀 레벨 해결
- **커밋 로그** — append-only 해시 체인
- **콘텐츠 주소 지정** — SHA-256 루트 해시 (무결성 검증용)

의도적으로 생략한 것: 확률적 청킹, 머클 트리 경로 증명, 온디스크 포맷.

### 4.3 Branch — 스냅샷 기반 격리

```rust
pub struct Branch {
    rig_id: RigId,
    data: Arc<BTreeMap<i64, WorkItem>>,  // main 스냅샷의 Arc clone
    base_commit: u64,
}
```

순수 스냅샷 모델: 브랜치는 분기 시점의 데이터만 본다. main에 이후 추가된 항목은 merge 시 합쳐진다.

### 4.4 충돌 해결

3-way merge: base(분기 시점) vs source(브랜치) vs dest(main).

| # | 상황 | 규칙 | 적용 필드 |
|---|------|------|----------|
| 1 | 한쪽만 고침 | 고친 쪽 반영 (OneSided) | 모든 필드 |
| 2 | 스칼라 양쪽 고침 | 나중에 쓴 쪽 (LWW, `updated_at` 비교) | claimed_by 등 |
| 3 | 배열 양쪽 고침 | 합치기 (G-Set, 중복 제거) | tags |
| 4 | status / priority | 더 높은 쪽 (MaxRegister) | status (Done > Claimed), priority (P0 > P1) |

```rust
pub trait Mergeable {
    fn merge(&self, other: &Self) -> Self;
    // 만족해야 하는 성질: 교환, 결합, 멱등
}
```

### 4.5 시스템 Rig

Board 초기화 시 자동 등록되는 시스템 rig:
- `"human"` — 사용자. CLI에서의 stamp은 human이 수행.
- `"evolver"` — Evolver. stamp 기반 스킬 생성 시 사용.

---

## 5. Rig 아키텍처

### 5.1 Strategy 패턴: WorkMode

Operator(대화)와 Worker(작업)는 **생성 로직을 공유하지만 런타임 행동이 다르다.** Strategy 패턴으로 이 차이를 캡슐화한다.

```rust
pub trait WorkMode: Send + Sync {
    fn session_for(&self, input: &WorkInput) -> String;
    fn session_config(&self, input: &WorkInput) -> SessionConfig { ... }
}

pub struct ChatMode { session_id: String }  // 영속 세션 → prompt cache 보장
pub struct TaskMode;                         // 작업당 새 세션
pub struct EvolveMode;                       // stamp 분석당 세션
```

### 5.2 Rig\<M> — Strategy를 사용하는 Context

```rust
pub struct Rig<M: WorkMode> {
    pub id: RigId,
    board: Option<Arc<Board>>,       // Operator는 None 가능
    agent: Agent,                    // Goose Agent
    mode: M,                         // Strategy
    cancel: CancellationToken,
    middleware: Vec<Arc<dyn Middleware>>,
}

pub type Operator = Rig<ChatMode>;   // 사용자 대화
pub type Worker = Rig<TaskMode>;     // Board pull loop
pub type Evolver = Rig<EvolveMode>;  // Stamp → skill 생성
```

**공유 로직** — `process()`는 모든 모드에서 동일:

```rust
impl<M: WorkMode> Rig<M> {
    pub async fn process(&self, input: WorkInput) -> anyhow::Result<()> {
        let session_config = self.mode.session_config(&input);
        let message = Message::user().with_text(&input.text);
        let stream = self.agent.reply(message, session_config, Some(self.cancel.clone())).await?;
        // stream 소비, conversation_log 기록
    }
}
```

### 5.3 Operator — 사용자 대화

```rust
impl Operator {
    pub fn without_board(id: RigId, agent: Agent, session_id: impl Into<String>) -> Self;
    pub async fn chat(&self, input: &str) -> anyhow::Result<()>;
    pub async fn chat_streaming(&self, input: &str) -> anyhow::Result<impl Stream<Item = ...>>;
}
```

Board를 거치지 않는 직접 대화. 영속 세션으로 prompt cache 보장.

### 5.4 Worker — Board Pull Loop

```rust
impl Worker {
    pub async fn run(&self) {
        // Phase 0: sweep_orphaned_worktrees (crash 복구)
        // Phase 1: 이전에 claimed된 작업 재개
        // Phase 2: pull loop
        loop {
            let notified = board.notify_handle().notified();
            match self.try_claim_and_execute(&repo_dir).await {
                Ok(true) => continue,    // 즉시 다음 작업 확인
                Ok(false) => {}          // 대기
                Err(e) => warn!(...),
            }
            tokio::select! {
                _ = notified => {}
                _ = self.cancel.cancelled() => break,
            }
        }
    }
}
```

**작업 실행 상세 (`process_claimed_item`):**

```
1. acquire_worktree — attach(기존) 또는 create(새)
2. middleware.on_start — 컨텍스트 사전 수집 (AGENTS.md, 스킬, Board prime)
3. resolve_session — 기존 세션 재개 또는 새 세션 생성
4. execute_with_retry:
   ├─ process(prompt) → Agent.reply()
   ├─ middleware.validate() → cargo check/test
   ├─ 통과 → submit
   ├─ 실패 + 재시도 가능 → fix prompt → retry (최대 2라운드)
   └─ 실패 + 재시도 초과 → mark_stuck (worktree 유지)
5. worktree 정리 (stuck이 아닌 경우)
```

### 5.5 미들웨어 파이프라인

```rust
#[async_trait]
pub trait Middleware: Send + Sync {
    async fn on_start(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<()> { Ok(()) }
    async fn validate(&self, ctx: &PipelineContext<'_>) -> anyhow::Result<Option<String>> { Ok(None) }
}

pub struct PipelineContext<'a> {
    pub agent: &'a Agent,
    pub work_dir: &'a Path,
    pub rig_id: &'a RigId,
    pub board: &'a Board,
    pub item: &'a WorkItem,
}
```

구현된 미들웨어:
- **ContextHydrator** — `on_start`: AGENTS.md + 스킬 카탈로그 + Board prime을 시스템 프롬프트에 주입
- **ValidationGate** — `validate`: Cargo.toml 있으면 `cargo check`, package.json 있으면 `npm test`

### 5.6 Git Worktree

```rust
pub struct WorktreeGuard {
    pub path: PathBuf,      // /tmp/og-rigs/{rig-id}/{work-id}/
    pub branch: String,     // rig/{rig-id}/{work-id}
    repo_dir: PathBuf,
    pub keep: bool,         // true면 Drop 시 정리 안 함 (Stuck용)
}
```

- `create()` — `git worktree add` 실행, 브랜치 생성
- `attach()` — 기존 worktree에 연결 (stale claim 재개용)
- `remove()` — async 정리 (`spawn_blocking`으로 블로킹 방지)
- `sweep_orphaned_worktrees()` — crash 복구용, 보드에 claimed 상태 없는 worktree 정리
- RAII: `Drop` 시 `keep == false`이면 자동 삭제

### 5.7 Prompt Caching 전략

Strategy 패턴이 캐싱을 구조적으로 보장:

| 경로 | 세션 | 캐시 동작 |
|------|------|-----------|
| Operator (ChatMode) | 영속 — 항상 같은 세션 | full prefix hit: system + tools + 전체 이력 |
| Worker (TaskMode) | 작업당 새 세션 | system + tools만 hit |

### 5.8 Board 도구 (MCP Platform Extension)

에이전트가 Board에 접근하는 두 가지 경로:

1. **MCP Platform Extension** — `BoardClient`가 `McpClientTrait` 구현. Goose의 도구 파이프라인 (보안, 퍼미션) 자동 상속. 인프로세스, JSON-RPC 오버헤드 없음.
2. **CLI 서브커맨드** — `opengoose board {status|ready|claim|submit|create|abandon|stamp}`. Skills로 에이전트에게 사용법을 알려줌.

---

## 6. Evolver — Stamp 기반 스킬 진화

Evolver는 stamp을 감시하고 낮은 점수의 작업을 분석하여 스킬을 자동 생성/업데이트한다.

```
stamp_notify.notified()
  │
  ▼
낮은 점수 stamp 발견 (threshold: 0.3)
  │
  ▼
LLM 분석 (Agent.reply) — 실패 원인 파악
  │
  ├─ SKILL.md 생성 → 새 스킬 등록
  ├─ UPDATE:{name} → 기존 스킬 업데이트
  └─ SKIP → 이미 알려진 교훈, 무시
  │
  ▼
stamp.evolved_at 마킹 (중복 처리 방지)
```

- **loop_driver** — `stamp_notify` 대기 + fallback 주기적 스캔 (5분)
- **sweep** — 미처리 stamp 일괄 스캔
- **pipeline** — 프롬프트 구성 → LLM 호출 → 응답 파싱 → 스킬 저장

---

## 7. Skills 시스템

`opengoose-skills` 크레이트는 Board, Rig, Goose에 의존하지 않는 독립 크레이트.

주요 모듈:
- **loader** — 파일시스템에서 `.md` 스킬 파일 로드
- **catalog** — 로드된 스킬 목록 관리
- **metadata** — YAML frontmatter 파싱 (name, description, triggers)
- **lifecycle** — active, deprecated 등 상태 관리
- **manage** — add, remove, list, update, promote, discover, lock CLI 핸들러
- **evolution** — LLM 응답 파싱, 스킬 검증, 파일 쓰기 (Evolver가 사용)

### 7.5 Sandbox — HVF microVM

`opengoose-sandbox`는 macOS Hypervisor.framework (HVF)를 사용하는 경량 microVM 샌드박스 크레이트. 다른 크레이트에 의존하지 않는 독립 크레이트로, Worker가 에이전트 코드를 격리 실행할 때 사용할 목적으로 설계되었다.

**핵심 컴포넌트:**

- **MicroVm** — CoW 메모리 매핑으로 스냅샷에서 fork한 VM 인스턴스. ARM64 vCPU, PL011 UART, VirtIO 콘솔 에뮬레이션.
- **SandboxPool** — 스냅샷 캐시(`OnceLock`) + VM 재사용(`Mutex<Option<MicroVm>>`). 첫 `acquire()`에서 스냅샷 생성, 이후 호출은 VM/vCPU reset으로 서브밀리초 재사용.
- **VmSnapshot** — vCPU 레지스터, 메모리 크기, 커널 해시, GIC/vtimer/VirtIO 상태를 bincode로 직렬화. 디스크 캐시 지원.

**현재 상태:** macOS ARM64 전용 (`#[cfg(target_os = "macos")]`). Worker 통합은 미완 — 크레이트 단독으로 VM 부팅 및 코드 실행까지 동작.

---

## 8. Beads 알고리즘

### 8.1 ready() — 작업 가능한 것

```rust
pub fn filter_ready(items: impl Iterator<Item = WorkItem>, blocked_ids: &HashSet<i64>) -> Vec<WorkItem> {
    // 1. open 상태만
    // 2. blocked_ids에 없는 것만
    // 3. 우선순위 정렬 (P0 > P1 > P2)
}
```

### 8.2 prime() — 에이전트 컨텍스트 요약

```rust
pub fn prime_summary(items: &[WorkItem], rig_id: &RigId) -> String {
    // Board: N open, M claimed, K done
    // Rig: {rig_id}
    // Recent: 최근 완료 3개
}
```

### 8.3 compact() — 메모리 감쇠

```rust
pub fn find_compactable(items: impl Iterator<Item = WorkItem>, older_than: Duration, now: DateTime<Utc>) -> Vec<WorkItem> {
    // 닫힌 상태 (Done, Abandoned, Stuck) + 임계값 이상 경과한 항목
}
```

---

## 9. 신뢰 모델 (Wasteland)

### 9.1 Stamps

```rust
// SeaORM 엔티티
pub struct Stamp {
    pub id: i64,
    pub target_rig: String,
    pub work_item_id: i64,
    pub dimension: String,        // Quality | Reliability | Helpfulness | 커스텀
    pub score: f32,               // -1.0 ~ +1.0
    pub severity: String,         // Leaf(1.0x) | Branch(2.0x) | Root(4.0x)
    pub stamped_by: String,       // ≠ target_rig (졸업앨범 규칙)
    pub comment: Option<String>,
    pub evolved_at: Option<DateTime<Utc>>,       // Evolver가 처리했는지
    pub active_skill_versions: Option<String>,   // stamp 시점의 스킬 버전
    pub timestamp: DateTime<Utc>,
}
```

**가중 점수 (시간 감쇠):**

```rust
fn stamp_weighted_value(stamp: &Stamp, now: DateTime<Utc>) -> f32 {
    let days = (now - stamp.timestamp).num_seconds() as f32 / 86400.0;
    let decay = 0.5_f32.powf(days / 30.0);  // 30일 반감기
    severity_weight * stamp.score * decay
}
```

### 9.2 신뢰 사다리

| 수준 | 가중 점수 | 능력 |
|------|----------|------|
| L1 (Newcomer) | < 3 | 작업 claim만 |
| L1.5 (Recognized) | >= 3 | + 하위 작업 생성 |
| L2 (Contributor) | >= 10 | + stamp 가능 |
| L2.5 (Trusted) | >= 25 | + 최상위 작업 생성 |
| L3 (Veteran) | >= 50 | + 무제한 위임 |

```rust
impl TrustLevel {
    pub fn from_score(score: f32) -> Self { ... }
}
```

### 9.3 졸업앨범 규칙 (Yearbook Rule)

자기 작업을 stamp할 수 없다. Board API 레벨에서 강제. 예외 없음.

```rust
BoardError::YearbookViolation { stamper, target }
```

### 9.4 차원별 점수

```rust
pub struct DimensionScores {
    pub quality: f32,
    pub reliability: f32,
    pub helpfulness: f32,
    pub other: f32,             // 커스텀 차원의 합산
}
```

---

## 10. CLI 인터페이스

### 10.1 실행 모드

```rust
pub enum RunMode {
    Tui,              // 기본. ratatui TUI (Chat | Board | Logs 탭)
    Headless,         // `opengoose run "..."` — 단일 작업 후 종료
    CliSubcommand,    // `opengoose board|rigs|skills|logs` — 서브커맨드
}
```

### 10.2 서브커맨드

```
opengoose                           # TUI 대화형 모드
opengoose run "task description"    # 헤드리스 모드

opengoose board status              # 보드 상태
opengoose board ready               # claim 가능한 작업
opengoose board claim <id>          # 작업 가져가기
opengoose board submit <id>         # 완료 제출
opengoose board create "title"      # 새 작업 게시
opengoose board abandon <id>        # 포기
opengoose board stamp <id> -q 0.8 -r 1.0 -p 0.6 --severity Branch

opengoose rigs                      # rig 목록
opengoose rigs add --id <id> --recipe <recipe>
opengoose rigs remove <id>
opengoose rigs trust <id>           # 신뢰 수준 조회

opengoose skills {add|remove|list|update|promote|discover}
opengoose logs {list|show|tail}
```

### 10.3 TUI

ratatui 기반 3탭 인터페이스:
- **Chat** — Operator와 대화 (`chat_streaming` → 토큰 단위 스트리밍)
- **Board** — 작업 목록, 상태, rig 현황
- **Logs** — tracing 로그 실시간 표시 (`TuiLayer` → mpsc 채널)

### 10.4 Web Dashboard

`opengoose` 시작 시 자동으로 웹 서버 기동 (기본 포트 1355):
- REST API: `/api/board`, `/api/rigs`, `/api/skills`
- SSE: 실시간 상태 업데이트
- HTML 대시보드 페이지

---

## 11. Runtime 와이어링

`runtime::init_runtime(port)` 에서 전체 시스템을 조립:

```rust
pub async fn init_runtime(port: u16) -> Result<Runtime> {
    // 1. Board 연결 (SQLite)
    let board = Board::connect(&db_url()).await?;

    // 2. Web 대시보드 시작
    web::spawn_server(board.clone(), port).await?;

    // 3. Evolver 시작 (stamp_notify 감시)
    tokio::spawn(evolver::run(board.clone(), stamp_notify));

    // 4. Worker 시작 (pull loop)
    let worker = Worker::new(id, board, agent, TaskMode, middleware);
    tokio::spawn(worker.run());

    Ok(Runtime { board, worker })
}
```

---

## 12. v1 → v0.2 마이그레이션

### 유지하는 것 (컨셉)

- 커스텀 CoW 스토어 + branch/merge
- Beads 알고리즘 (ready/prime/compact)
- Stamps + 신뢰 사다리 + 졸업앨범 규칙
- 에이전트 통신용 MCP 도구

### 버리는 것

- 21개 중 17개 크레이트
- 커스텀 Engine, GatewayBridge, StreamResponder
- Push 기반 TeamOrchestrator (Chain/FanOut/Router)
- Profile 시스템 (Goose Recipe로 직접 대체)
- 모든 플랫폼 게이트웨이 (Discord, Slack, Telegram, Matrix)
- Federation (나중으로 연기)
- AgentPool, Deacon, ReviewQueue, DelegationTracker

### 추가된 것

| 측면 | 원래 설계 | 현재 |
|------|----------|------|
| 크레이트 수 | 3개 | 6개 (opengoose-evolver, opengoose-sandbox 추가) |
| WorkMode | ChatMode, TaskMode | + EvolveMode (Evolver용) |
| Rig 타입 | Operator, Worker | + Evolver |
| 데이터 레이어 | 인메모리만 (Phase 1) | SQLite + CowStore 듀얼 |
| UI | CLI만 계획 | TUI (ratatui) + Web 대시보드 |
| 스킬 시스템 | 없음 | opengoose-skills 크레이트 전체 |
| 스킬 진화 | 없음 | Evolver (stamp → skill 자동 생성) |
| 로깅 | 없음 | TuiLayer + 파일 로테이션 |
| Worktree | 계획만 | WorktreeGuard (RAII, sweep, attach/resume) |

---

## 13. 설계 결정 기록

### ADR-1: Board의 SQLite + CowStore 단일 소유

Board struct는 SQLite(영속성)와 CowStore(인메모리 브랜치/머지) 두 저장소를 소유한다.
`merge()` 메서드에서 staged clone → merge → persist → swap 4단계가 하나의 Mutex lock 안에서 실행되어 원자성을 보장한다.
persist 실패 시 swap이 안 일어남 → CowStore와 SQLite 일관성 자동 보장.
분리하면 이 원자성을 외부 호출자가 보장해야 하므로 동기화 버그 표면적이 증가한다.
재검토 시점: board.rs가 500줄을 넘거나, 저장소 백엔드를 교체할 필요가 생길 때.

---

## 14. 열린 질문

1. ~~**대화가 보드를 우회해야 하는가?**~~ **해결됨 (§ 2.3).** Operator가 직접 처리.
2. **Federation 범위?** 전면 연기. v0.2 = 단일 인스턴스.
3. **WorkItem 확장 필드?** `project`, `parent`, `session_id`, `seq`, `assigned_to`, `notes`, `result` — Phase 후반.
4. ~~**샌드박스 추상화?**~~ **부분 해결.** `opengoose-sandbox` 크레이트로 HVF microVM 구현 (§ 7.5). Worker 통합은 아직 미완.
5. **멀티 Worker CLI UX?** 현재 단일 Worker. 복수 Worker 시 동시 스트림 표시 전략 미정.
6. **경험 기억 (Layer 2)?** 설계됨 (원본 ARCHITECTURE.md § 4.5) 하지만 미구현. `board__remember`/`board__recall` 도구, 시간 감쇠, pre-compaction flush 등.
7. **Portless 프록시?** 설계됨 (원본 § 5.8) 하지만 미구현. 복수 rig가 동시에 dev 서버 실행 시 필요.

---

## 14. 의도적 보류

검토했으나 현 시점에서 의도적으로 보류한 결정들.

### 14.1 evolver 크레이트 분리 보류

`evolver/` (2,490 LOC)가 `crate::skills`에 의존하므로 분리 시 skills도 함께 나가야 한다. 현재 모듈 경계가 깨끗하므로 규모가 더 커질 때 재검토.

### 14.2 Board struct 리팩토링 보류

CowStore는 이미 별도 타입(`store/mod.rs`)이고 Board가 위임하는 구조. 현재 규모에서 추가 분리는 불필요.

### 14.3 runtime 에러 핸들링 현행 유지

`unwrap_or_else`로 cwd 폴백은 합리적. Worker 생성 실패 시 전체 init 실패는 의도된 동작 — Worker 없이 운영 불가.
