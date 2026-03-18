# 참조 프로젝트 — v0.2 설계 결정의 근거

> OpenGoose v0.2 아키텍처에 영향을 준 외부 프로젝트 조사.

---

## 1. Goose (block/goose)

**무엇인가:** Block이 만든 Rust 기반 AI 에이전트 프레임워크. MCP-native, 스트림 우선.

**v0.2를 위한 핵심 교훈:**

- **`Agent::reply()`는 `BoxStream<AgentEvent>`를 반환** — 항상 스트리밍, 동기 경로 없음. 우리의 rig가 이 스트림을 직접 소비한다.
- **MCP 우선** — 모든 도구가 MCP 서버. 독자 도구 포맷 없음. 우리의 팀 도구(`board__*`)는 MCP Stdio 서버.
- **Recipe 시스템** — extension, instruction, parameter, sub-recipe가 포함된 YAML 에이전트 설정. v1의 `AgentProfile` + `ProfileStore`를 대체.
- **SessionManager** — SQLite 기반 대화 이력 + fork/export. 전부 재사용.
- **ExtensionManager** — MCP 클라이언트 수명주기, 도구 라우팅, 도구 네이밍. 건드리지 않음.
- **서브에이전트 시스템** — `Agent::new()`가 격리된 인스턴스 생성. 병렬 워커에 적합하지만 부모-자식만 가능. 우리의 보드 기반 pull 모델이 P2P에 더 유연.
- **에러 핸들링** — 에러를 도구 결과로 모델에 전송 (errors-as-prompts). 모델이 자기 수정. 이것을 상속.
- **자동 압축** — 80% 임계값에서 컨텍스트 요약. 이것을 상속.

**재구현하지 않을 것:**

| Goose가 처리 | 우리가 추가 |
|--------------|------------|
| LLM 호출 루프 + 재시도 | Wanted Board (pull 작업 분배) |
| MCP 도구 디스패치 | Board MCP 도구 (`board__*`) |
| 세션 영속성 | 작업 항목 영속성 (CoW 스토어) |
| 컨텍스트 관리 | Prime 컨텍스트 주입 |
| 퍼미션 모드 | 신뢰 기반 능력 게이팅 |
| 에러 복구 | Witness (stuck/zombie 감지) |

**최소 통합:**
```rust
// Goose와 하는 것은 이것이 전부
let agent = Agent::new(recipe, extensions).await;
let stream = agent.reply(message, session_config, cancel_token).await;
// AgentEvent를 위해 스트림 소비
```

---

## 2. Dolt (dolthub/dolt)

**무엇인가:** Git 시맨틱을 가진 MySQL 호환 SQL 데이터베이스. prolly tree로 구축.

**채택한 핵심 컨셉:**

### Prolly Tree
- 모든 노드가 콘텐츠 주소 지정된(SHA-256) B-tree
- 이력 독립: 삽입 순서와 무관하게 같은 데이터 → 같은 트리 구조
- 경계 안정 노드를 위한 콘텐츠 기반 청킹
- **v0.2 적용:** CoW 시맨틱의 `Arc<BTreeMap>` 사용. 진정한 prolly tree는 아니지만 중요한 속성을 보존: O(1) 브랜치, O(d) diff, 콘텐츠 주소 루트 해시.

### Branch/Merge 시맨틱
- `dolt branch` = O(1) 포인터 생성 → 우리의 `board.branch()` = Arc clone
- `dolt commit` = 루트 해시 스냅샷 → 우리의 `board.commit()` = 해시 + 로그 항목
- `dolt merge` = 3-way merge (base vs source vs dest) → 같은 알고리즘
- `dolt diff` = 변경된 경로만 비교 → 우리의 diff는 변경된 키를 비교

### 셀 레벨 충돌 해결
- Dolt는 행 레벨이 아닌 셀 레벨(행 + 열)로 머지
- 에이전트 A가 열 X를, 에이전트 B가 같은 행의 열 Y를 변경 → 충돌 아님
- 같은 (primary_key, column)을 다른 값으로 변경한 경우만 → 충돌
- **v0.2 필드 전략:** SourceWins, DestWins, HigherStatusWins, LatestTimestamp, Immutable, Union

### 생략하는 것
- 확률적 청킹 (대규모 온디스크 트리에 필요, 인메모리에는 불필요)
- 머클 경로 증명 (연합 검증에 필요, Phase 2로 연기)
- SQL 쿼리 엔진 (직접 키-값 접근 사용)
- 온디스크 포맷 (내구성은 SQLite WAL)

---

## 3. Beads (steveyegge/beads)

**무엇인가:** AI 에이전트를 위한 분산 그래프 이슈 트래커 및 영속 메모리 시스템. Dolt 위에 구축.

**채택한 핵심 컨셉:**

### 해시 기반 ID
```
SHA256(prefix + "|" + title + "|" + description)[:6]
→ "bd-a1b2c3"
```
동시 에이전트 생성에서도 충돌 저항성. 중앙 ID 권한 불필요.

### 3대 핵심 알고리즘

**`ready()`** — 열린 블로킹 의존성이 없는 작업, 우선순위 정렬.
- 사전 계산된 `blocked_cache`로 O(1) 준비성 확인
- 이행적 블로킹: A가 B를 막고 B가 C를 막으면 A가 이행적으로 C를 막음

**`prime()`** — 세션 시작을 위한 1-2K 토큰 컨텍스트 요약.
- 우선순위 분포, 블로킹 이슈, 준비된 작업, 최근 완료
- `BriefIssue`/`BriefDep` 모델 사용: 전체 객체 대비 97% 토큰 절감
- 세션 시작 시 시스템 프롬프트에 주입, 매 턴 갱신

**`compact()`** — 에이전트 메모리 감쇠.
- 30일 이상 된 닫힌 항목 → AI 생성 요약이 전체 내용 대체
- 보존: hash_id, title, status, relationships, stamps
- 삭제: 상세 설명, 수락 기준, 로그

### Wisps (임시 작업)
세 가지 상태: Proto (고체, 동결 템플릿) → Molecule (액체, 활성, 영구) → Wisp (기체, 휘발, 동기화 없음).
- Wisp는 burn (하드 삭제), squash (요약), promote (영구화) 가능
- 영구 기록을 오염시키지 않는 에이전트 스크래치 작업

### 의존성 그래프
6가지 관계 유형: blocks, parent-child, waits-for, relates-to, duplicates, supersedes.
- 순환 감지 포함 이행적 블로킹 계산
- `waits-for` 게이트: FanOut 완료 (모든/일부 자식 완료)

### 3-way Merge 드라이버 (필드 레벨)
- 스칼라: 타임스탬프 기준 last-write-wins
- 배열 (labels, deps): 중복 제거 union
- 상태: 우선순위 규칙 (closed > in_progress > open)
- 우선순위: 수치적 최대 (P0 > P1)

### Beads가 SQLite에서 Dolt로 이동한 이유
1. 쓰기 경합 — SQLite의 단일 작성자 잠금이 5+ 동시 에이전트에서 실패
2. 셀 레벨 머지 — SQLite는 전체 행 덮어쓰기; Dolt는 열 단위 머지
3. 이력 — 즉시 롤백을 위한 `SELECT ... AS OF`
4. 동기화 — 자동 머지가 포함된 `dolt push/pull`

**v0.2 참고:** 단일 바이너리 Rust를 원하기 때문에 CoW BTreeMap (Dolt 아님)을 유지. 스케일링이 필요하면 Board API 뒤에서 Dolt로 저장소 백엔드 교체 가능.

---

## 4. Wasteland (steveyegge/wasteland)

**무엇인가:** 분산 에이전트 연합 프로토콜. Gas Town 위의 스케일아웃 레이어.

**채택한 핵심 컨셉:**

### Wanted Board (Pull 기반 작업 분배)
상태 머신: `open → claimed → in_review → completed`
- 누구나 작업 게시 가능 (승인 게이트 없음)
- 에이전트가 자율적으로 탐색하고 claim (push가 아닌 pull)
- 오픈 현상금 모드: 여러 에이전트가 병렬로 작업, 첫 번째 유효한 솔루션이 승리

**v0.2:** 보드가 곧 우리의 Wanted Board. 모든 작업이 이를 통과한다.

### Stamps (다차원 평판)
- 차원: Quality (1-5), Reliability (1-5), Creativity (파생)
- 심각도 가중치: leaf=1pt, branch=3pt, root=5pt
- 모든 stamp은 근거가 있는 특정 완료 작업을 참조
- Append-only 장부 (이력 재작성 불가)

### 신뢰 사다리 (점진적 자율성)
L1 (< 3pt) → L1.5 (≥ 3) → L2 (≥ 10) → L2.5 (≥ 25) → L3 (≥ 50)
- 자연스러운 도제 과정: 좋은 작업 수행 → stamps 누적 → 결국 다른 이를 stamp

### 졸업앨범 규칙 (Yearbook Rule)
"다른 사람의 졸업앨범에는 서명할 수 있지만, 자기 것에는 안 된다."
- DB 레벨에서 `stamped_by != target_rig` 강제
- 평판은 다른 이가 당신에 대해 쓰는 것

### GUPP (추진 원칙)
"Hook에 작업이 있으면 실행해야 한다." 작업이 있을 때 에이전트는 절대 idle하지 않는다.
- 우리의 witness가 GUPP 위반 감지: 보드에 claim 가능한 작업이 있는데 에이전트가 idle

### Federation (Phase 2로 연기)
- 이식 가능한 ID를 위한 HOP URI: `hop://alice@example.com/rig-id/`
- Dolt를 통한 fork 기반 모델: upstream commons → fork → local clone → PR 반환
- 크로스 인스턴스 stamp 동기화

---

## 5. Portless (vercel-labs/portless)

**무엇인가:** 포트 번호를 네임드 URL로 대체하는 로컬 프록시.

**채택한 컨셉 (아이디어만 — Portless 바이너리 자체는 사용하지 않음):**

우리는 Portless의 URL 네이밍 철학과 worktree 감지 아이디어를 차용하되, `portless.rs`에서 자체 구현한다. Portless의 프록시 데몬(포트 1355)은 사용하지 않는다.

### 포트 대신 네임드 URL
```
이전: http://localhost:3000, http://localhost:3001, http://localhost:8080
이후: http://myapp.localhost, http://api.localhost, http://docs.localhost
```

- Portless 원본: 프록시 데몬이 포트 1355에서 서브도메인을 랜덤 백엔드 포트로 라우팅
- `PORT` 환경변수 주입 (프레임워크가 자동으로 인식)
- 프로그래밍적 URL 발견을 위한 `PORTLESS_URL` 환경변수

### Git Worktree 자동 감지
Portless가 git worktree를 자동 감지하고 브랜치 이름을 접두사로 추가:
- 메인: `http://myapp.localhost:1355`
- Worktree `fix-ui`: `http://fix-ui.myapp.localhost:1355`

### 쿠키/스토리지 격리
각 서브도메인이 자체 쿠키 저장소와 localStorage 범위를 가짐. 다른 브랜치에서 작업하는 rig 간 세션 누출 없음.

**v0.2 적용:**
- 각 rig가 코드 작업용 git worktree를 받음
- 각 worktree가 네임드 URL 받음: `{rig-id}.{project}.localhost`
- 병렬 rig 간 포트 충돌 구조적으로 불가능
- 에이전트가 `PORTLESS_URL` 환경변수로 자기 URL 발견

**생략하는 것:**
- Portless 프록시 데몬 (자체 구현으로 대체 — 외부 바이너리 의존성 회피)
- Turborepo/Next.js 특화 기능 (우리는 프레임워크 무관)
- HTTPS 인증서 자동 생성 (로컬 개발 전용이므로 HTTP로 충분)

---

## 6. Gas Town / Goosetown (맥락)

### Gas Town (steveyegge/gastown)
Mayor → Witness → Deacon → Polecats → Refinery 아키텍처.
- 75k LOC Go, 17일간 vibecoding
- 핵심 교훈: "설계가 병목" — 에이전트가 구현을 처리하면 아키텍처 결정이 제한 요소가 됨
- **Landing the Plane의 원 출처** — Beads가 데이터 모델을 제공하지만, 세션 종료 프로토콜 자체는 Gas Town에서 처음 구현됨

**Landing the Plane (세션 종료 프로토콜, Gas Town 기원):**
1. **FILE** — 미완료 작업을 태스크로 기록
2. **GATE** — 품질 검사 실행 (lint, test)
3. **UPDATE** — 완료 항목 닫기, 진행 중 항목 주석
4. **SYNC** — git push (비협상)
5. **VERIFY** — 깨끗한 작업 트리 확인
6. **HANDOFF** — `ready()`로 다음 작업 선택 (Beads 알고리즘 사용)

**가져오는 것:** Witness 패턴 (stuck/zombie), Landing the Plane, "re-imagine" 머지 충돌, GUPP 원칙.
**가져오지 않는 것:** Tmux 기반 프로세스 격리, Go 코드베이스, Mail 시스템.

### Goosetown (block/goosetown)
Block의 미니멀 Gas Town 변형. Conductor → Instruments.
- gtwall: bash 파일 기반 append-only 브로드캐스트 (~400줄)
- Village Map: 에이전트 애니메이션이 있는 시각적 대시보드

**가져오는 것:** 단순함 철학 (개념당 파일 하나), 보드를 통한 소통 영감.
**가져오지 않는 것:** Push 모델 (Conductor가 작업 할당).

---

## 7. Production Agent Systems (Open SWE + Stripe + Ramp + Coinbase)

> 이 섹션은 네 개 프로덕션 시스템의 공통 패턴을 통합 정리한다.
> 개별 시스템의 고유 통찰은 하단 참고표에 별도 기재.

**무엇인가:** Open SWE (langchain-ai)는 Stripe Minions, Ramp Inspect, Coinbase Cloudbot의 프로덕션 패턴을 재사용 가능한 코딩 에이전트 아키텍처로 체계화한 오픈소스 프레임워크.

### 채택한 공통 패턴

**미들웨어 훅 (Deep Agents 패턴, Open SWE 체계화)**

결정론적 로직 주입을 위한 4개 수명주기 지점:

```
before_agent(state)          -- 1회 초기화
wrap_model_call(fn, state)   -- 각 LLM 호출 감싸기
before_tool_call(call, state) -- 도구 실행 전
after_tool_call(call, msg, state) -- 도구 실행 후
```

+ `@before_model` (메시지 큐 주입) + `@after_agent` (안전망).

**v0.2 적용:** `on_start()`, `pre_hydrate()`, `post_execute()`가 있는 `Middleware` 트레잇. Deep Agents보다 단순 (Goose가 내부적으로 호출별 래핑을 처리하므로 불필요).

**블루프린트 패턴 (Stripe Minions 기원, Open SWE 일반화)**

결정론적 노드 (고정 코드: git 연산, 린팅, PR 생성)와 에이전트 노드 (LLM 추론) 교차. 결정론적 노드는 토큰을 절약하고 예측 가능.

**v0.2 적용:** Rig의 `execute()` 메서드가 곧 블루프린트 — 결정론적 사전 수집 → 에이전트 루프 → 결정론적 검증 → 결정론적 커밋/PR.

**컨텍스트 사전 수집 (Stripe 기원)**

에이전트 루프 전, 결정론적으로 티켓/스펙 참조에 대해 도구 실행:
- 레포 루트에서 `AGENTS.md` 읽기
- 연결된 이슈 내용 fetch
- 검색으로 관련 코드 사전 로드
- 에이전트가 빈 상태가 아닌 풍부한 컨텍스트로 시작

**v0.2 적용:** `ContextHydrator` 미들웨어가 워크스페이스 파일 + AGENTS.md 읽기 + 코드 검색 사전 실행.

**안전망 PR (Open SWE)**

`@after_agent` 훅이 에이전트 루프 후 커밋되지 않은 변경사항을 감지, 자동으로 브랜치 + 커밋 + PR 생성. 에이전트가 잊어도 작업 손실 방지.

**v0.2 적용:** `post_execute()`의 `SafetyNet` 미들웨어.

**제한된 CI 루프 (Stripe + Open SWE 공통)**

CI 최대 2라운드. 이후 needs-human-review로 표시. 무한 재시도의 수확체감.

**큐레이션된 도구 세트 (Stripe Toolshed)**

에이전트가 전체 도구가 아닌 **부분 집합**을 받음. Stripe의 Toolshed는 ~500개 도구가 있지만 각 minion은 ~15개만 봄. Open SWE 기본값도 ~15개 도구.

**v0.2 적용:** Goose Recipe가 rig별 extension 설정을 정의. 연구자는 `git push`를 받지 않음. 개발자는 `slack_reply`를 받지 않음.

**샌드박스 격리 (Ramp + Stripe 공통)**

각 에이전트 세션이 완전 격리된 환경에서 실행 (Ramp의 Modal 샌드박스, Stripe의 EC2 devbox). 파일시스템 스냅샷으로 사전 워밍.

**v0.2 적용:** rig별 Git worktree (로컬). 향후 Docker/Modal/Daytona 지원을 위한 `SandboxBackend` 트레잇.

### 개별 시스템 고유 통찰

| 시스템 | 핵심 통찰 | 고유 패턴 |
|--------|-----------|-----------|
| **Stripe Minions** — 주당 1,300+ PR. Goose 포크. | "모델이 시스템을 운영하는 게 아니다. 시스템이 모델을 운영한다." | 디렉토리별 범위 지정 규칙 파일 (Cursor 포맷), 사전 워밍 Devbox (10초 스핀업) |
| **Ramp Inspect** — 전체 머지 PR의 ~30%. Modal 샌드박스. | 전체 컨텍스트 샌드박스 = 에이전트가 인간 엔지니어와 같은 도구를 가짐. | 30분마다 파일시스템 스냅샷 재구축, 사용자 타이핑 시 워밍 프리로딩, 전후 스크린샷 비교 |
| **Coinbase Cloudbot** — 엔터프라이즈 프레임워크. | 관찰 가능성과 감사 가능성은 경성 요구사항. | 코드 우선 그래프 아키텍처, 결정론적 노드는 유닛 테스트 / LLM 노드는 eval 하니스, 모든 도구 호출 추적 + diff |

---

## 요약: 설계 계보

```
Dolt (prolly tree, branch/merge, 셀 레벨 diff)
  → 우리의 CoW 스토어 + 3-way merge에 영향

Beads (ready/prime/compact, wisps, 해시 ID, 의존성 그래프)
  → 우리의 Wanted Board 데이터 모델 + 알고리즘에 영향

Wasteland (pull 아키텍처, stamps, 신뢰, yearbook, federation)
  → 우리의 pull 루프 + 신뢰 모델 + 보드 설계에 영향

Gas Town (witness, GUPP, landing the plane, re-imagine 머지)
  → 우리의 운영 패턴에 영향

Goose (Agent::reply(), MCP, Recipes, Sessions)
  → 우리의 에이전트 런타임 그 자체 (재구현하지 않음)

Portless (네임드 URL, worktree 감지) — 컨셉만 차용, 바이너리 미사용
  → 우리의 rig 격리에 영향 (포트 충돌 없음)

프로덕션 시스템 (Open SWE + Stripe + Ramp + Coinbase):
  미들웨어 훅, 블루프린트 패턴, 컨텍스트 사전 수집, 안전망 PR
  → 우리의 rig 실행 파이프라인에 영향
```
