# OpenGoose v0.2 — Q&A

> 설계 과정에서 나온 질문과 답변, 그리고 리서치 결과.

---

## 1. "Rig"라는 명칭은 어디서 나온 거야?

**Wasteland** (Steve Yegge)에서 온 용어. Wasteland에서 모든 참여자 (인간, AI 에이전트, Gas Town 인스턴스)를 Rig라고 부른다. Mad Max 세계관의 차량/장비에서 유래.

Wasteland의 `rigs` 테이블 스키마:
```sql
handle VARCHAR(255) PRIMARY KEY
display_name VARCHAR(255)
hop_uri VARCHAR(512)
trust_level INT DEFAULT 0
rig_type VARCHAR(16) DEFAULT 'human'
parent_rig VARCHAR(255)
```

**왜 Agent가 아닌 Rig인가:**
- Goose의 `Agent` struct와 이름 충돌 방지
- Goose `Agent` = LLM 호출 단위 (한 번의 reply)
- 우리의 `Rig` = Agent 위에 정체성 + pull 루프 + 신뢰를 얹은 상위 개념
- 이름이 다른 게 오히려 역할 구분이 명확

---

## 2. Goose의 MCP 우선 시스템을 가져가는 게 좋을까?

**Yes. 단, Goose의 Platform Extension (내장 방식)을 사용한다.**

Goose에는 두 가지 인프로세스 확장 메커니즘이 있다:

| 메커니즘 | 통신 방식 | 오버헤드 | 용도 |
|----------|----------|---------|------|
| **Builtin** (`ExtensionConfig::Builtin`) | DuplexStream 위 MCP JSON-RPC | 직렬화 비용 있음 | 외부 MCP 서버를 번들링할 때 |
| **Platform** (`ExtensionConfig::Platform`) | `McpClientTrait` 직접 구현 | **제로** (Rust vtable 호출) | 인프로세스 도구 |

**Platform Extension이 우리에게 맞는 이유:**
- 별도 프로세스/바이너리 불필요 (단일 바이너리 목표 달성)
- MCP JSON-RPC 직렬화 오버헤드 없음
- Goose의 도구 검사 파이프라인 (보안, 퍼미션) 자동 상속
- `PlatformExtensionContext`로 SessionManager, ExtensionManager 접근 가능

**구현 패턴:**

```rust
pub struct BoardClient {
    info: InitializeResult,
    board: Arc<Board>,  // 보드에 대한 공유 참조
}

#[async_trait]
impl McpClientTrait for BoardClient {
    async fn list_tools(&self, ...) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult { tools: Self::get_tools(), .. })
    }

    async fn call_tool(&self, ctx: &ToolCallContext, name: &str,
                       arguments: Option<JsonObject>, ...) -> Result<CallToolResult, Error> {
        match name {
            "claim_next" => { /* board.claim() */ }
            "create_task" => { /* board.post() */ }
            "update_status" => { /* board.update() */ }
            "stamp" => { /* board.stamp() */ }
            _ => { /* unknown tool */ }
        }
    }
}
```

등록:
```rust
// PLATFORM_EXTENSIONS에 등록 (Goose 시작 시)
PlatformExtensionDef {
    name: "board",
    display_name: "Board",
    default_enabled: true,
    unprefixed_tools: false,  // board__claim_next 형태로 노출
    client_factory: |ctx| Box::new(BoardClient::new(ctx, board_arc)),
}
```

**Goose의 기존 Platform Extension 목록 (참고):**

| 이름 | unprefixed | 역할 |
|------|-----------|------|
| developer | true | write, edit, shell, tree |
| analyze | true | tree-sitter 코드 분석 |
| todo | false | 작업 추적 (세션 상태) |
| summon | true | 서브에이전트 위임 |
| tom | false | top-of-mind 컨텍스트 주입 |

---

## 3. Goose Recipe의 한계는 없을까?

**있다. Recipe는 정적 설정이므로 동적 요소는 Rig 레이어에서 처리.**

| 한계 | 설명 | 대응 |
|------|------|------|
| 정적 설정 | YAML 파일, 런타임 변경 어려움 | prime()을 시스템 프롬프트에 동적 주입 |
| 신뢰 수준 미반영 | "L2만 이 도구 사용" 개념 없음 | Rig가 trust level에 따라 extension 목록 동적 구성 |
| 워크스페이스 컨텍스트 | instructions는 정적 문자열 | pre_hydrate 미들웨어가 파일 읽어서 합성 |
| 팀 조율 | sub_recipes는 부모-자식 순차/병렬만 | sub_recipes 미사용, 보드가 조율 |

**결론:** Recipe = 에이전트의 기본 설정 (모델, provider, 기본 extension). 동적인 것 (prime, trust 기반 도구 큐레이션, 워크스페이스 컨텍스트) = Rig 레이어.

---

## 4. Goose의 SessionManager는 사용하는 게 좋을까?

**대화 이력에는 Yes, 작업 추적에는 No.**

| 영역 | 담당 | 이유 |
|------|------|------|
| 대화 이력 | Goose SessionManager | SQLite 기반, fork/export/resume 지원. 재구현 불필요. |
| 작업 수명주기 | Board CoW store | open → claimed → done. SessionManager와 별개. |

관계:
```
Rig가 대화형 작업 claim
  → Goose SessionManager: 세션 생성/재개 (대화의 연속성)
  → Board: work item 상태 업데이트 (작업의 수명주기)
```

---

## 5. Portless 등 에이전트 간 격리는 어떻게 구현할 거야?

**Portless 컨셉을 단일 바이너리에 내장 프록시로 구현.**

세 겹의 격리:

**Layer 1: 데이터 격리 (Board branch)**
```
Rig A → board.branch("rig-a") → 독립 CoW 스냅샷
Rig B → board.branch("rig-b") → 서로 간섭 없음
완료 → board.merge() → 3-way 셀 레벨 머지
```

**Layer 2: 코드 격리 (Git worktree)** — 코드 작업 시에만
```
git worktree add /tmp/og-rigs/researcher/bd-a1b2 -b rig/researcher/bd-a1b2
→ 에이전트의 working_dir = 이 worktree
→ 완료 시 main에 머지 + worktree 삭제
```

**Layer 3: 네트워크 격리 (내장 프록시)** — dev 서버 실행 시에만
```
opengoose 시작 시 → EmbeddedProxy 실행 (포트 1355)

Rig가 dev 서버 시작:
  1. 프로젝트 포트 자동 감지 (package.json, docker-compose 등)
  2. 프록시에 등록: developer → [{web: 3000}, {api: 8080}]
  3. 환경변수 주입:
     OPENGOOSE_URL=http://developer.localhost:1355
     OPENGOOSE_API_URL=http://developer.localhost:1355/api

브라우저/에이전트 접근:
  developer.localhost:1355/api/users → localhost:8080/users 로 프록시
```

**자세한 내용:** [ARCHITECTURE.md § 5.5](ARCHITECTURE.md#55-git-worktree--내장-프록시)

---

## 6. 에이전트는 어떻게 구현하고 어떻게 관리할 거야?

**구현 구조:**

```
┌─ Rig ───────────────────────────────────────┐
│  id: RigId (안정적, 디스크에 영속)           │
│  recipe: "researcher"                        │
│  trust: L2                                   │
│  ┌─ Goose Agent ──────────────────────────┐ │
│  │  Agent::reply() (LLM 호출 루프)        │ │
│  │  ExtensionManager (MCP 도구)           │ │
│  │  SessionManager (대화 이력)            │ │
│  └────────────────────────────────────────┘ │
│  board__* tools (Platform Extension, 내장)   │
│  Pull loop:                                  │
│    board.wait_for_claimable()                │
│    → claim → execute → submit → merge        │
└──────────────────────────────────────────────┘
```

**관리:**

```yaml
# ~/.opengoose/rigs.yaml
rigs:
  - id: researcher-01
    recipe: researcher
    auto_start: true
  - id: developer-01
    recipe: developer
    auto_start: true
```

CLI:
```bash
opengoose rigs                          # 목록 + 상태 + 신뢰
opengoose rigs add --recipe developer   # 새 rig 등록
opengoose rigs remove developer-01      # rig 삭제
```

---

## 7. 에이전트의 범위가 어떻게 돼?

| 차원 | 범위 결정 |
|------|----------|
| 도구 | Recipe의 extension 설정. 연구자는 웹검색+파일읽기만, 개발자는 +셸+git. |
| 작업 유형 | Board의 `ready()` 필터. Recipe 태그와 work item 태그 매칭. |
| 권한 | Trust level. L1=claim만, L3=stamp 가능. |
| 코드 접근 | Worktree 내부 한정. main 직접 수정 불가. |
| 컨텍스트 | Goose 세션으로 격리. 다른 rig의 대화 불가. |
| 통신 | board__* 도구만. P2P 아니라 보드 통한 간접 통신. |

---

## 8. sccache는 사용하고 있는 거야?

sccache가 설치되어 있었지만 (`/opt/homebrew/bin/sccache`) 활성화되지 않았다.

**v0.2에서 프로젝트 레벨로 활성화:**
```toml
# .cargo/config.toml
[build]
rustc-wrapper = "/opt/homebrew/bin/sccache"
```

Goose 의존성 (git fetch + 컴파일)이 무거우므로 반복 빌드 시 효과적.

---

## 9. 지금 어디까지 된 거고 이제 뭘 하면 되는 거야?

**완료:**
- v0.2 워크트리 생성 (branch: `v0.2`)
- v1 전부 삭제 (125,900줄)
- 아키텍처 + 참조 프로젝트 + QNA 문서 (한국어)
- 3개 크레이트 뼈대
- sccache 활성화

**다음 (로드맵):**

| Phase | 내용 | 산출물 |
|-------|------|--------|
| **1. Board** | 데이터 레이어 | WorkItem 타입, CowStore, Board API, Branch/Merge, SQLite 영속성, 테스트 |
| **2. Rig** | 에이전트 레이어 | Rig struct, pull 루프, Goose Agent 통합, Board Platform Extension (내장), Witness |
| **3. CLI** | 사용자 인터페이스 | 대화형 REPL, 헤드리스 `run`, `--clean` 플래그 |
| **4. Beads + Trust** | 조율 + 평판 | ready/prime/compact, Stamps, 태그 매칭, 신뢰 사다리 |

---

## 10. Wasteland에서 Rig는 구체적으로 어떻게 동작하는가? (리서치 결과)

### 10.1 Rig 수명주기

**생성:** `wl join` 명령으로 자기 등록.
1. 업스트림 commons DB를 DoltHub에 fork
2. fork를 로컬에 clone
3. `rigs` 테이블에 자기 레코드 INSERT (멱등)
4. PR 모드: 브랜치 push + PR 생성
5. 설정 파일 저장 (`~/.config/wasteland/...`)

**소멸:** 명시적 destroy 없음. `wl leave`로 로컬 설정/데이터 삭제. `rigs` 테이블 레코드는 영구 보존 (append-only, 감사 가능).

**정체성:** HOP URI (`hop://email@domain/handle/`). 모든 mutation, completion, stamp에 첨부. 복수 wasteland 참여 가능 — stamp이 따라감.

### 10.2 작업 분배: 완전한 자기 선택

Wasteland 프로토콜에 **디스패처/스케줄러가 없다**. 완전히 자기 선택:

```
Rig가 wl browse → 열린 항목 목록 확인
Rig가 wl claim <id> → status=open 검증 → claimed_by=<handle> 설정
  PR 모드: wl/<handle>/<item-id> 브랜치에 커밋 → push → PR
  Wild-west 모드: main에 직접 커밋 → push
Rig가 wl done <id> --evidence <url> → completion 레코드 생성 → in_review
다른 Rig가 wl accept → stamp 생성 (author ≠ subject 강제)
```

**퍼미션:**
- 게시자: claim, unclaim, delete, update, accept (단, 자기 작업 accept 불가)
- Claim한 rig: unclaim, done 가능; 자기 제출물 accept 불가
- 기타: 미할당 항목만 claim 가능

### 10.3 동시성: 한 Rig가 여러 작업 가능

**Wasteland 프로토콜 레벨:** 한 rig가 여러 항목을 동시에 claim할 수 있다. 각 claim은 자체 브랜치 `wl/<handle>/<item-id>`에서 작업하므로 충돌 없음.

**Gas Town 레벨:** 20-30+ 병렬 에이전트 (Polecat) 동시 실행.
- 각 Polecat이 자체 git worktree에서 작업
- Refinery가 순차적으로 리베이스/머지 처리
- 실효성 있는 병렬 한계: rig당 ~5개 Polecat (이후 수확체감)

**SDK 내부:** `mutate()` 함수에 mutex (`c.mu.Lock()`)로 단일 프로세스 내 직렬화. 하지만 다른 항목에 대한 동시 claim은 프로토콜 레벨에서 허용.

### 10.4 작업 분해: Wasteland가 아닌 Gas Town 레벨

**Wasteland 프로토콜은 작업 분해를 하지 않는다.** Wanted Board는 평면 목록. 각 wanted 항목은 원자적.

작업 분해는 Gas Town (로컬 오케스트레이터)에서 발생:
```
인간 → Mayor에게 지시
  → Mayor가 MEOW (Molecular Expression of Work)로 분해
  → Formula (TOML 템플릿) → Molecule (실행 중 인스턴스) → Bead (원자 작업)
  → gt sling으로 Polecat에 dispatch
  → Polecat이 개별 bead 실행
  → 완료된 작업을 Wasteland에 evidence로 게시
```

**OpenGoose v0.2 적용:**
- Board = Wasteland의 Wanted Board (평면, 원자적 항목)
- 작업 분해 = Rig 레이어 (에이전트가 board__create_task로 하위 작업 생성)
- Mayor 역할 = 사용자 또는 특정 "orchestrator" rig

### 10.5 팀 개념: Wasteland에 없음

Wasteland에 명시적 "팀" 개념이 없다. Gas Town 인스턴스 전체 (Mayor + Witness + Polecat 5-30개)가 **단일 rig**로 등록.

`parent_rig` 컬럼으로 계층 관계 가능하지만, 프로토콜 레벨에서 팀 조율은 없음. 조율은 Wanted Board 자체가 담당: 게시 → claim → 완료 → 검증 흐름이 자연스러운 협업 패턴.

**OpenGoose v0.2 적용:**
- 팀 = 같은 보드를 공유하는 rig 그룹 (명시적 팀 개념 불필요)
- 조율 = 보드가 담당 (게시/claim/완료/stamp)
- 한 OpenGoose 인스턴스가 여러 rig를 spawn (Gas Town이 여러 Polecat을 spawn하는 것과 동일)

### 10.6 세션 관리: Wasteland는 무상태

Wasteland 프로토콜에 세션/대화 컨텍스트 개념이 없다. 각 CLI 명령 (`wl claim`, `wl done`)이 독립 실행. 상태는 전부 Dolt DB에.

Gas Town에서는 풍부한 세션 관리:
- tmux 세션으로 에이전트 격리
- GUPP: "Hook에 작업이 있으면 실행해야 한다"
- `/handoff`로 컨텍스트 가득 차면 새 세션에 이관
- `gt seance`로 이전 세션에 질의

**OpenGoose v0.2 적용:**
- 세션 = Goose SessionManager (대화 이력)
- 작업 상태 = Board (Wasteland처럼 무상태 프로토콜)
- GUPP = Witness가 감시 (보드에 작업 있는데 idle이면 경고)

---

## 11. 프로젝트 개념이 없는데, "foo 추가해줘"라고 하면 어디서 해?

**Rig와 프로젝트는 독립적인 축.** Rig는 프로젝트에 대해 모른다 — 작업 항목이 프로젝트를 알고 있고, rig는 claim한 작업의 컨텍스트를 받아서 사용할 뿐.

**프로젝트 컨텍스트 = "코드 작업할 때 어디서 하는지".** git repo 안에서 `opengoose`를 실행하면 자동 감지. 대화가 프로젝트에 관한 건지 범용인지는 LLM이 판단. 분류 로직 없음.

```bash
# git repo 안 → 프로젝트 컨텍스트 있음 (코드 접근 가능)
$ cd ~/dev/myapp && opengoose
> JWT 만료 처리가 어떻게 돼 있어?       # 코드 읽기 가능
> 일반적으로 rate limiting은 뭐가 있어?  # LLM이 범용으로 답변
> /task "rate limiting 추가"              # worktree 생성

# git repo 밖 → 프로젝트 없음 (코드 작업 불가)
$ cd ~ && opengoose
> /task "뭔가 구현해줘"                  # 에러: "프로젝트를 지정해주세요"
```

"git repo 안에서 실행"과 "프로젝트 없이 실행"을 따로 구분하지 않음. 프로젝트 컨텍스트가 **있는** 상태일 뿐이고, 코드 작업이 필요할 때만 해당 디렉토리를 사용.

**참조:** Wasteland은 프로젝트 개념 없음 (Wanted Board는 평면 목록). Gas Town에서 "rig = git repo" 바인딩이었지만, 이는 한 rig를 여러 프로젝트에서 쓸 수 없는 한계가 있음. v0.2에서는 작업 항목에 프로젝트가 붙고 rig는 어떤 프로젝트의 작업이든 claim할 수 있는 구조.

---

## 12. Rig별 데이터는 어떻게 관리해?

**데이터가 세 계층으로 나뉜다: Global / Per-Rig / Per-Project.** 자세한 내용은 [ARCHITECTURE.md § 4.1](ARCHITECTURE.md#41-데이터-계층-global--per-rig--per-project) 참조.

Per-Rig 데이터는 Global도 아니고 Project도 아닌 **rig 자체에 속하는 데이터:**
- 현재 상태 (idle, working, stuck, zombie)
- 작업 이력 (completions — 뭘 했고 어떻게 끝났는지)
- respawn 횟수 (circuit breaker — 너무 많이 죽으면 차단)
- 통계 (완료율, 평균 소요 시간, 실패율)

**참조 시스템:**
- **Gas Town Agent Bead:** 각 Polecat이 `issues` 테이블에 영속 레코드. 현재 상태, hook (현재 작업), exit_type, respawn 횟수 누적. CV chain = Dolt 커밋 이력.
- **Wasteland completions:** 각 completion이 `completed_by` (rig handle)을 기록. Per-rig 작업 이력이 자연스럽게 쌓임.
- **Beads:** `prime()`은 에이전트별 개인화 없음 (프로젝트별). 에이전트 귀속은 `assignee`, `created_by`, `actor`, `closed_by_session` 컬럼으로 추적.

---

## 13. 에러 핸들링은 어떻게 해?

**세 가지 레벨의 에러 핸들링:**

### 13.1 Goose 레벨 (자동)
Goose의 errors-as-prompts 패턴 상속:
- 도구 실패 → 에러 메시지를 LLM에 전달 → 모델이 자기 수정
- 네트워크 일시 실패 → 자동 재시도 (Goose 내장)

### 13.2 Rig 레벨 (Witness)
```rust
pub enum RigStatus {
    Idle,
    Working { work_id: HashId, started_at: DateTime<Utc> },
    Stuck { work_id: HashId, reason: String },  // 타임아웃 또는 반복 실패
    Zombie,                                      // 프로세스 응답 없음
}
```

Witness가 감지하는 상황:
- **Stuck**: 작업이 N분 이상 진행 중 (기본: 30분)
- **Zombie**: heartbeat 없음 (기본: 5분)
- **GUPP 위반**: 보드에 claim 가능한 작업이 있는데 idle

대응:
- Stuck → 작업을 `needs-human-review`로 표시, rig respawn
- Zombie → rig 강제 종료 + respawn (circuit breaker: 연속 3회 실패 시 비활성화)

### 13.3 Board 레벨 (복구)
```
프로세스 충돌
  → 재시작
  → SQLite WAL에서 최신 상태 복구
  → claimed 상태였던 작업 → open으로 롤백 (다른 rig가 claim 가능)
```

---

## 14. 태그 매칭은 어떻게 동작해?

**규칙:**
- 작업에 태그가 있으면: rig의 recipe 태그와 **완전 일치** 필요
- 작업에 태그가 없으면: 아무 rig나 claim 가능

**예시:**
```
work_item { tags: ["researcher"] }
  → researcher recipe rig만 claim 가능

work_item { tags: [] }
  → main, researcher, developer 누구나 claim 가능

work_item { tags: ["researcher", "senior"] }
  → researcher + senior 태그를 모두 가진 rig만 claim 가능
```

**근거:** Stripe Toolshed (에이전트별 도구 부분집합) + Wasteland (태그 없으면 자유 claim) 조합.

---

## 15. Stamp 시스템은 어떻게 동작해?

**Stamp 구조:**
```rust
Stamp {
    dimension: Quality | Reliability | Helpfulness,  // 3차원
    score: f32,        // -1.0 ~ +1.0
    severity: Severity, // Leaf(1.0x) | Branch(2.0x) | Root(4.0x)
    timestamp: DateTime<Utc>,
}
```

**점수 계산:**
```
weighted_score = Σ(severity_weight × score × time_decay)
time_decay = 0.5^(days_elapsed / 30)  // 30일 반감기
```

**CLI에서 stamp 주기:**
```bash
# 작업 완료 후 자동 프롬프트
✓ bd-a1b2 완료 — "rate limiting 구현"
  평가하시겠습니까? [Y/n] y
  품질 (-1.0 ~ +1.0): 0.8
  중요도 (leaf/branch/root): branch
  ✓ Stamp 기록됨

# 또는 명시적 명령
> /stamp bd-a1b2 quality:0.8 branch
> /stamp bd-a1b2 reliability:1.0 root

# 빠른 승인 (기본값: quality:0.8, leaf)
> /approve bd-a1b2

# 빠른 거절 (stamp 없이 재작업 요청)
> /reject bd-a1b2 "테스트가 불충분함"
```

**제재:** 가중 점수가 -5.0 이하 → read-only 모드 (읽기만 가능)

**자세한 내용:** [ARCHITECTURE.md § 8](ARCHITECTURE.md#8-신뢰-모델-wasteland)
