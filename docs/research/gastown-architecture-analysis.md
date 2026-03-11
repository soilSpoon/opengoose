# Gastown & Goosetown 아키텍처 분석 리서치

> 작성일: 2026-03-11
> 소스: Steve Yegge Medium 블로그, Maggie Appleton 분석, Block Goose 블로그, GitHub 레포지토리 분석

---

## 1. 개요: "Gastown"이란 무엇인가?

**Gastown**은 Steve Yegge가 2026년 1월 1일에 발표한 **멀티 에이전트 오케스트레이션 패러다임**이다. AI 코딩 에이전트(주로 Claude Code) 20~30개를 병렬로 조율하여 대규모 소프트웨어 개발 작업을 수행하는 시스템을 말한다.

> "Gas Town은 복잡하다. 내가 원해서가 아니라, 자급자족하는 기계가 될 때까지 계속 컴포넌트를 추가해야 했기 때문이다." — Steve Yegge

이 패러다임을 구현한 두 프로젝트가 존재한다:

| 프로젝트 | 저장소 | 성격 |
|---------|--------|------|
| **Gas Town** | `steveyegge/gastown` | 엔터프라이즈급 풀스택 구현 (Go, 300+ 파일) |
| **Goosetown** | `block/goosetown` | 미니멀 연구 중심 구현 (Python/Bash, Goose 기반) |

Maggie Appleton의 분석에 따르면, Gas Town은 "진지하게 사용하는 생산 도구"라기보다는 **"도발적인 추측적 설계 허구(speculative design fiction)"**에 가깝다. 미래 에이전트 시스템의 형태를 스케치하는 프로토타입이라는 평가다.

---

## 2. 왜 멀티 에이전트인가? — 패러다임 전환

### 2.1 순차적 → 병렬 에이전트 사용의 전환

대부분의 사람들은 AI 에이전트를 **순차적으로** 사용한다:

```
1:00 PM — "API 엔드포인트 만들어줘" → [10분 대기]
1:10 PM — "프론트엔드 만들어줘"     → [10분 대기]
1:20 PM — "테스트 작성해줘"         → [10분 대기]
1:30 PM — 프로젝트 완료 (총 30분)
```

일부 엔지니어들이 깨달은 것: **에이전트 하나를 돌릴 수 있으면, 동시에 다섯 개도 돌릴 수 있다.**

- Agent A: API 구축
- Agent B: 프론트엔드 시작
- Agent C: 테스트 작성
- Agent D: 레거시 코드베이스 버그 조사

> "이것이 사람들이 시간을 되사는 방법이다. 한 시간 안에 스프린트 전체를 끝낸다." — Block/Goose 블로그

### 2.2 병렬화가 만드는 새로운 문제

그러나 에이전트간 통신이 없으므로 새로운 문제가 발생한다:

1. **머지 충돌**: 두 에이전트가 같은 파일의 같은 라인을 수정
2. **컨텍스트 소실**: 세션 크래시 또는 환각(hallucination)으로 한 시간의 "작업"이 사라짐
3. **인간 병목**: 주말에도 에이전트가 제대로 동작하는지 계속 확인하는 **에이전트 베이비시터**가 됨

**Gas Town과 Goosetown은 바로 이 "베이비시팅"을 없애기 위해 설계되었다.**

---

## 3. 철학과 핵심 원칙

### 3.1 공통 철학

#### Research-First, Build-Second (조사 먼저, 구현 나중)
- 모든 비자명한 작업은 병렬 리서치로 시작한다
- "서류 위의 놀라움이 코드 위의 놀라움보다 저렴하다"
- 3개 소스에서 80% 신뢰도면 충분히 진행

#### Propulsion Principle (추진 원칙)
- 에이전트는 즉시 실행한다. 질문하지 않고, 기다리지 않고, 서론 없이 바로 작업
- "물리학이지, 예절이 아니다: 모든 지연의 순간은 시스템이 멈추는 순간"
- 현재 모델들은 "도움이 되는 어시스턴트"로 훈련되어 정중하게 대기하는 습성이 있어, 이를 극복하기 위해 **공격적 프롬프팅과 지속적 nudging**이 필요

#### Context is Finite (컨텍스트는 유한하다)
- 오케스트레이터는 자신의 컨텍스트를 사수해야 한다
- 방향 설정(오케스트레이터)과 작업 실행(위임자)을 엄격히 분리
- 산출물을 만드는 모든 작업은 위임한다
- Context rot은 컨텍스트 윈도우 한계 이전에도 출력 품질을 저하시킴

#### Write as You Go (가면서 기록하라)
- 워커와 라이터는 점진적으로 산출물을 만든다
- 매 도구 호출마다 디스크에 일관된 부분 산출물을 남긴다
- 취소된 에이전트의 8/10 섹션 > 메모리에만 있던 완전한 작업

### 3.2 Gas Town 고유 원칙

#### Attribution is Not Optional (귀속은 선택이 아니다)
모든 행위에는 행위자가 있다:
```
Git commits:    gastown/crew/joe <owner@example.com>
Beads records:  created_by: gastown/crew/joe
Event logs:     actor: gastown/crew/joe
```

#### Work is Data (작업은 데이터다)
- 티켓이 아닌 완전한 출처 추적(provenance)
- 감사 추적(audit trail)과 컴플라이언스 기록
- 역량 기반 라우팅 가능

#### NDI (Nondeterministic Idempotence, 비결정적 멱등성)
- 잠재적으로 불안정한 프로세스의 오케스트레이션을 통해 유용한 결과를 도출
- 영속적 Beads + 감시 에이전트로 최종적 완료를 보장

### 3.3 Maggie Appleton이 식별한 핵심 통찰

#### "디자인이 병목이 된다"
> "에이전트 무리가 코드 작업을 처리할 때, 개발 시간은 더 이상 병목이 아니다. 디자인이 제한 요소가 된다: 무엇을 만들고 싶은지 상상하고, 그 상상을 현실로 만드는 데 필요한 세세한 디테일을 파악하는 것."

에이전트가 대신할 수 없는 것들:
- 아키텍처 결정
- 사용자 경험 감각
- 우선순위 판단
- 비전과 취향

#### "에이전트 코딩의 최대 함정"
> "너무 빨리 움직여서 생각할 틈이 없다. 프롬프트가 너무 쉬워서, 각 단계에서 무엇을 만들고 있는지 충분히 고려하지 않는다. 구조적 결정의 늪, 알 수 없는 버그, 원래 무엇을 만들려 했는지 흐릿한 기억 속에 허리까지 빠져야, 10억 토큰을 뜨거운 쓰레기 더미와 교환했다는 것을 깨닫게 된다."

---

## 4. 아키텍처 상세

### 4.1 Gas Town (steveyegge/gastown) — 엔터프라이즈 아키텍처

```
┌─────────────────────────────────────────────────────────────┐
│                      TOWN (도시) 레벨                        │
│                                                             │
│  Mayor 🎩 ─── 인간 컨시어지, 절대로 코드를 쓰지 않음           │
│  Deacon ──── 데몬 감시견, 연속 순찰 사이클                    │
│  Boot ────── Deacon의 감시견 (5분마다 Deacon 생존 확인)        │
│  Dogs ────── 유지보수 에이전트 (압축, 건강 체크, 아카이브)       │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                      RIG (작업대) 레벨                        │
│                                                             │
│  Witness ──── Polecat 건강 모니터링, 정체된 워커 nudge         │
│  Refinery ─── 머지 큐 관리 (Bors 스타일 batch-then-bisect)    │
│  Polecats 🦨 ─ 임시 그런트 워커 (단일 작업 후 소멸)             │
│  Crew ────── 인간 워크스페이스                                │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                    인프라스트럭처 레벨                         │
│                                                             │
│  Dolt SQL Server (포트 3307) ─── Git 시맨틱의 SQL DB          │
│  Git Worktrees ─── Polecat별 격리된 작업 공간                 │
│  Beads ─── Git 기반 원자적 작업 단위                          │
│  Mail ─── 영속적 메시징 시스템                                │
│  Tmux ─── 터미널 멀티플렉싱                                  │
│  Dashboard ─── 실시간 웹 UI (포트 8080)                      │
└─────────────────────────────────────────────────────────────┘
```

**기술 스택:** Go 1.25, Dolt (MySQL 프로토콜), Charmbracelet TUI, gRPC, Cobra CLI

#### 계층적 감독 체계 (Hierarchical Supervision)

```
       [You / Human]
            │
        [Mayor] ─────── 절대로 코드를 쓰지 않음. 지시와 조율만.
            │
    ┌───────┼───────────────────┐
    │       │                   │
[Witness] [Deacon]          [Dogs]
    │       │               (유지보수)
    │    [Boot]
    │    (Deacon 감시)
    │
[Polecats] ──── 실제 코드 작성하는 워커들
    │
[Refinery] ──── 완료된 작업의 머지 큐 관리
```

핵심 포인트:
- **Mayor는 절대 코드를 쓰지 않는다** — 인간과 대화하고, 작업을 만들어 워커에 배정
- **감시자 계층이 주기적 심장박동(heartbeat)처럼 순환** — 누군가 정체되면 감지하여 nudge
- "Claude Code에서 Gas Town으로 가면, 짝 프로그래밍에서 대규모 엔지니어링 리더십으로 올라간다"

#### 핵심 작업 흐름 (GUPP 원칙: "Hook에 작업이 있으면, 반드시 실행하라")
1. Mayor 또는 인간이 Convoy(작업 배치)를 생성
2. `gt sling <bead-id> <rig>` — 작업을 에이전트 Hook에 배정
3. 에이전트가 `gt hook`으로 Hook 감지
4. 즉시 실행 (대기 없음)
5. `gt done` — 완료 제출 후 idle 전환
6. Refinery가 머지 큐 처리
7. Witness가 좀비/정체 상태 모니터링

#### Polecat 생명주기 — 영속 ID, 임시 세션
- **Identity** (영구) — 에이전트 Bead, CV 체인, 작업 이력
- **Sandbox** (배정간 영속) — Git worktree
- **Session** (임시) — Claude 컨텍스트 윈도우 (자유롭게 kill & respawn)
- 상태: `WORKING → (handoff cycles) → IDLE → (next sling) → WORKING`
- **Seancing (강신술)**: 새 세션이 이전 세션을 별도 인스턴스로 부활시켜 미완료 작업에 대해 질문

#### 머지 큐 (Refinery) — Bors 스타일
1. **Batch**: 모든 MR을 main 위에 스택으로 rebase
2. **Test**: 스택 tip에서 테스트 실행
3. **PASS** → 전체 fast-forward merge
4. **FAIL** → 이진 탐색으로 범인 찾기
5. 충돌이 심할 경우 구현을 **"재상상(re-imagine)"** — 원래 의도를 유지하며 새 코드베이스에 맞게 재작성
6. 필요시 인간에게 에스컬레이션

> Maggie Appleton은 Gas Town에 없는 대안으로 **Stacked Diffs** 방식을 제안. Cursor의 Graphite 인수가 이 방향의 근거.

#### Dolt — "게임 체인저"

> "Dolt는 Git 시맨틱을 가진 SQL 데이터베이스다. Fork, branch, merge, pull request — 구조화된 데이터에 대해. 이것이 전체 연합(federation) 트릭을 작동하게 한다."

- Dolt가 SQLite/JSONL 백엔드의 모든 잔버그를 제거
- 스키마 마이그레이션이 용이 (Git처럼 버전 관리)
- 모델들이 Git을 잘 알기 때문에 Dolt도 빠르게 학습

**데이터 생명주기:** `CREATE → LIVE → CLOSE → DECAY → COMPACT → FLATTEN`

### 4.2 Goosetown (block/goosetown) — 미니멀 연구 아키텍처

```
┌──────────────────────────────────────────────────────────────┐
│                  Orchestrator (메인 세션)                      │
│        컨텍스트 관리, Flock 생성, 결과 합성                     │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐        │
│  │ Researchers  │  │   Workers    │  │  Reviewers   │        │
│  │  (3~6 병렬)  │  │  (병렬 실행)  │  │  (품질 게이트) │        │
│  │              │  │              │  │              │        │
│  │ · Local      │  │ · 코드 작성   │  │ · 보안       │        │
│  │ · GitHub     │  │ · 파일 생성   │  │ · 정확성     │        │
│  │ · Reddit     │  │ · 기능 구현   │  │ · 통합 검증   │        │
│  │ · StackOverflow│ │             │  │              │        │
│  │ · arXiv      │  │              │  │              │        │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘        │
│         │                 │                 │                │
│         └─────────────────┴─────────────────┘                │
│                           │                                  │
│                     ┌─────┴─────┐                            │
│                     │  gtwall   │  ← append-only 브로드캐스트  │
│                     └───────────┘                            │
│                                                              │
│  + Writers (합성 전문가) — 리서치 → 문서 변환                   │
└──────────────────────────────────────────────────────────────┘
```

**기술 스택:** Python 3.11+, Bash, FastAPI/Starlette, lit-html, Goose CLI

#### 탄생 배경

> "Goosetown은 Goose의 subagent 업그레이드를 한계까지 밀어붙이기 위한 재미있는 실험으로 시작됐다. Gas Town에서 시간을 보내다가, 훨씬 덜 방대한 변주가 background subagent를 보여주는 기발한 방법이 될 거라 생각했다. 일상 도구로 사용하기 시작하자 얼마나 잘 작동하는지에 완전히 놀랐다." — Tyler Longwell

#### 4가지 핵심 컴포넌트

1. **Skills** — 역할별 작업 방법을 기술하는 마크다운 파일. Delegate 생성 시 역할에 맞는 skill이 "pre-loaded"됨 (Orchestrator, Researcher, Writer, Reviewer)

2. **Subagents** — `summon` 확장의 `delegate()` 함수로 생성되는 임시 에이전트 인스턴스. 자체 클린 컨텍스트에서 작업하고 요약을 반환. 메인 세션을 빠르고 집중된 상태로 유지

3. **Beads** — Git 기반 로컬 이슈 트래커. Orchestrator가 이슈를 생성하고, delegate가 업데이트. 세션 실패 시 다음 에이전트가 bead를 집어 이어서 작업

4. **gtwall** — append-only 로그. 모든 delegate가 활동을 게시하고 읽음. 유일한 에이전트간 통신 채널

#### gtwall 실제 사용 예시

```
[10:14] researcher-api-auth - 🚨 잠재적 쇼스토퍼: 서비스 호출자의 capabilities가 비어있음.
        계획된 인증 경로가 모든 요청을 조용히 거부할 것. 설정이 아닌 코드 변경 필요.
[10:14] researcher-endpoints - 💡 발견: 최소 의존성의 네이티브 엔드포인트가 이미 존재.
        대안 경로 실행 가능.
[10:15] researcher-source - ✅ 완료. 확인: 네이티브 경로는 새 의존성 제로.
        피봇 권장.
```

→ 병렬 리서처들이 독립적으로 조사하다가, 쇼스토퍼를 발견하고, 대안을 찾고, 피봇을 결정하는 과정이 **1분 안에** 수렴.

#### 핵심 구조적 차이점
- **플랫 계층** — Delegate가 다른 Delegate를 생성할 수 없음
- **방송 조율, 공유 상태 없음** — 에이전트는 메모리나 도구 출력을 공유하지 않음
- **모델 유연성** — 기본 subagent 모델을 저렴한 것으로 설정 가능, ad-hoc 모델 선택도 지원

#### 워크플로우 5단계
1. **Research** — 3~6 병렬 리서처가 다양한 소스 탐색, gtwall에 발견사항 게시
2. **Process** — 결과 통합, 갭 식별, 필요시 후속 리서치 생성
3. **Plan & Dispatch** — 리서치 합성 → 계획, 워커/라이터 배정 (분리된 파일)
4. **Review** — 다차원 품질 리뷰 (crossfire: 다중 모델 적대적 QA)
5. **Synthesize** — 모든 조각을 통합, 리뷰어 발견사항 반영, 충돌 해결

> Tyler: "서브에이전트들이 서로 수다를 떠는 걸 보는 것은 눈이 번쩍 뜨이는 경험이었다. 그리고 웃기기도 했다. 마치 내가 동료들과 하듯이 러버덕 디버깅을 하고 의견을 주고받았다. 단일 모델이라도 다른 컨텍스트를 가진 여러 에이전트 형태로 아이디어를 자동으로 주고받게 하면 결과물이 더 좋아진다."

---

## 5. 핵심 컴포넌트 상세

### 5.1 Beads — 작업 추적의 원자 단위

Beads는 Yegge가 Gas Town 이전에 만든 메모리 관리 시스템으로, 두 프로젝트의 기반이다.

> Anthropic의 2025년 11월 논문 "장기 실행 에이전트를 위한 효과적인 하네스"에서도 에이전트 메모리 외부에 JSON 등 구조화된 형태로 원자적 작업을 추적하는 동일한 패턴을 제안. Claude Code에도 곧 이 유형의 작업 추적이 도입될 것으로 예상. — Maggie Appleton

| 속성 | Gas Town | Goosetown |
|------|----------|-----------|
| 저장소 | Dolt SQL Server | 로컬 Git 기반 |
| 스키마 | v6 (issues, dependencies, labels, comments, events, interactions, metadata) | 경량 이슈 트래커 |
| 레벨 | Town-level (`hq-*`) + Rig-level (`gt-*`, `bd-*`) | 단일 레벨 |
| 용도 | 전체 작업 생명주기 + 에이전트 ID/CV 관리 | 크래시 복구, 진행 추적 |

### 5.2 에이전트간 통신

**Goosetown의 gtwall** (~400줄 Bash):
- 불변 로그 + 리더별 위치 추적
- 원자적 쓰기 (lock directory, stale lock 감지)
- 포맷: `timestamp|sender_id|message` (파이프 이스케이프)
- 리서처는 3~5 도구 호출마다 발견사항 게시

**Gas Town의 Mail 시스템:**
- `issue_type='message'` Beads + 의존성 기반 스레딩
- 세션 재시작 간 영속
- `gt mail send`, `gt mail inbox`, `gt mail read`

### 5.3 Worktrees — 에이전트 격리

두 프로젝트 모두 **Git Worktrees**를 사용하여 각 에이전트를 격리:
- 각 에이전트가 자신만의 별도 작업 공간에서 작업
- 동일 파일을 동시에 수정하는 머지 충돌을 구조적으로 방지
- 전체 clone이 아닌 worktree로 효율적

---

## 6. "The Wasteland" — 연합(Federation) 비전

Steve Yegge의 2026년 3월 4일 블로그 "Welcome to the Wasteland: A Thousand Gas Towns"은 Gas Town의 다음 진화 단계를 제시한다.

### 6.1 핵심 컨셉

> "AI 도구의 모든 폼팩터 혁신은 100배의 토큰 지출 증가를 수반한다. Gas Town을 100배 확장하는 방법은? 수백 명의 Gas Town 사용자를 **연합(federation)**시켜 함께 구축하는 것."

- **Wanted Board** — 공유 작업 게시판. 누구나 게시 가능, 승인 게이트 없음. 아이디어, 작업, 버그, 기능, 리서치 질문, 문서, 디자인 무엇이든 가능
- **작업 생명주기**: `open → claimed → in review → completed`
- **Open-bounty 작업**: 아무도 claim하지 않고, 여러 rig이 병렬로 작업, 유효한 솔루션이 제출되면 종료

### 6.2 세 가지 행위자

| 행위자 | 역할 |
|--------|------|
| **Rigs** | 모든 참여자. 인간에게 롤업됨. AI측은 에이전트/Gas Town/다른 오케스트레이터. 핸들, 신뢰 레벨, 작업 이력 보유 |
| **Posters** | 작업을 게시판에 올리는 자 |
| **Validators** | 완료된 작업의 품질을 증명하는 자. 충분한 신뢰 레벨 필요 |

→ 고정된 역할이 아님. 어떤 rig이든 작업을 게시할 수 있고, 충분한 신뢰가 있으면 검증할 수 있음.

### 6.3 Stamps — 다차원 평판 시스템

**Stamp은 단순한 pass/fail이 아니다.** 다차원 증명(attestation):

| 차원 | 설명 |
|------|------|
| **Quality** | 작업의 품질 점수 |
| **Reliability** | 신뢰성 점수 |
| **Creativity** | 창의성 점수 |
| **Confidence** | 검증자의 확신 정도 |
| **Severity** | 리프 작업인지 루트 아키텍처 결정인지 |

- 특정 completion(특정 증거)에 앵커링됨 → 평판이 항상 실제 작업으로 추적 가능
- **Yearbook Rule (졸업앨범 규칙)**: "자기 작업에 도장을 찍을 수 없다. 평판은 타인이 당신에 대해 쓴 것이지, 스스로 주장하는 것이 아니다. LinkedIn과의 근본적 차이."

### 6.4 Trust Ladder — 신뢰 사다리

```
Level 3: Maintainer ─── 타인의 작업을 검증(stamp)할 수 있음
    ↑
Level 2: Contributor ── 검증된 기여 이력
    ↑
Level 1: Registered ─── 브라우징, claim, completion 제출 가능
```

→ 자연스러운 **도제 과정(apprenticeship path)**: 좋은 작업 → 스탬프 축적 → 타인을 스탬프하는 사람으로 승격

### 6.5 연합(Federation) 모델

- **탈중앙화**: 누구든 자신만의 Wasteland를 생성 가능 (팀, 회사, 대학, 오픈소스 프로젝트)
- **동일 스키마의 독립 데이터베이스**: 각 Wasteland는 주권적 DB
- **포터블 ID**: Rig 신원이 Wasteland 간 이동 가능. 스탬프가 따라감
- **Append-only, 버전 관리**: 이력을 다시 쓸 수 없음 — 영구 원장

> "작업이 유일한 입력이고, 평판이 유일한 출력이다. 평판을 살 수 없고, 팔로워 수를 게이밍할 수 없고, 증거와 분리된 사회적 신호가 없다."

### 6.6 반치트 설계

- 스탬프 그래프의 **형태(topology)**를 분석
- 공모 링(collusion ring)은 특징적 토폴로지를 가짐 — 상호 스탬핑이 많고, 경계가 뚜렷하고, 외부 비평가가 없음
- "사기를 불가능하게 만드는 것이 아니라, **비수익적으로** 만드는 것"

### 6.7 실적 및 팀

- 2개월 만에 **2,400 제출 PR, 1,500 병합, 450+ 고유 기여자**
- 여러 새 모델 세대가 나왔지만 Gas Town의 **아키텍처는 전혀 변형되지 않음** — 놀라운 회복력

| 인물 | 역할 |
|------|------|
| **Julian Knutsen** (ex-CashApp/Block/Bitcoin) | Wasteland 구현, #1 Gas Town 기여자 |
| **Dr. Matt Beane** (*The Skill Code* 저자) | 스킬/멘토링 시스템, GitHub 데이터로 초기 10,000 캐릭터 시트 생성 |
| **Chris Sells** | gastownhall.ai, Discord 커뮤니티 (Flutter를 100k→3M으로 성장시킨 인물) |
| **Tim Sehn** (DoltHub 창업자/CEO) | Beads/Dolt 기능/버그 지원 |
| **Brendan Hopper** | 분산 시스템 아키텍트, 연합 모델 비전/로드맵 |
| **Dane Poyzer** | Discord 커뮤니티 리더 |
| **Krystian Gebis** | 멀티 모델 지원 |
| **Pierre-Alexandre Entraygues** | OpenTelemetry |
| **Matt Wilkie** | Beads 다작 기여자, 공동 메인테이너 예정 |

### 6.8 Gas City — 다음 단계

> "Gas Town을 구성 요소로 해체하여 LEGO처럼 조합해 자신만의 오케스트레이터 토폴로지를 만들 수 있게 한다. Gas Town의 순수 선언적(declarative) 버전으로 교체하는 것이 목표."

---

## 7. 비판적 분석 — "코드와의 거리" 논쟁

### 7.1 Yegge의 입장: "100% 바이브코딩"

> "100% 바이브코딩이다. 코드를 본 적도 없고, 볼 생각도 없다."

Gas Town은 17일, 75k 라인, 2000 커밋으로 만들어졌다. Yegge의 두 번째 Claude 계정으로 Anthropic의 지출 한도를 우회. 월 $2,000~$5,000 추정, $GAS 크립토코인의 $75,000 거래 수수료로 충당.

### 7.2 외부 평가

**HN qcnguy:**
> "Beads는 좋은 아이디어에 나쁜 구현이다. 설계된 제품이 아니라, 의식의 흐름을 직접 코드로 변환한 것이다. 바이브코딩된 것뿐 아니라, **바이브 디자인**된 것이다. Gas Town은 그것을 만 배로 곱한 것이다."

**astrra.space (Bluesky):**
> "mayor는 돌멩이만큼 멍청하고, witness는 정기적으로 보는 것을 잊고, deacon은 자기 규칙을 만들고, crew는 금붕어 수조만큼의 대상 영속성을 가지고, polecat은 프로젝트에 최대한의 혼돈을 끼치려는 의도로 보인다. 이건 최고의 오락이다."

### 7.3 Maggie Appleton의 "코드 거리" 프레임워크

**코드에서 얼마나 멀리 물러서야 하는가?** 이분법이 아닌 **컨텍스트별 판단**:

| 요소 | 코드에 가까이 | 코드에서 멀리 |
|------|--------------|--------------|
| **도메인** | 프론트엔드/CSS (미적 감각 필요) | CLI/백엔드 (pass/fail 검증 가능) |
| **피드백 루프** | 정의하기 어려운 성공 기준 | 테스트/스크린샷으로 자체 검증 가능 |
| **위험 허용도** | 의료/금융 (규제 컴플라이언스) | 개인 블로그/사이드 프로젝트 |
| **프로젝트 유형** | Brownfield (축적된 관습) | Greenfield (실패 비용 낮음) |
| **협업자 수** | 팀 (코딩 표준/리뷰 파이프라인 필요) | 솔로 (YOLO 가능) |
| **경험 수준** | 주니어 (알려지지 않은 미지 취약) | 시니어 (패턴 인식 가능) |

### 7.4 비용 분석

| 항목 | 월 비용 | 연 비용 |
|------|---------|---------|
| 저렴한 Gas Town | $1,000 | $12,000 |
| 비싼 Gas Town | $3,000 | $36,000 |
| 미국 시니어 개발자 급여 | $10,000 | $120,000 |

→ 시니어 개발자의 **10~30% 비용**으로 2~3배 속도 향상이 가능하다면, 경제적으로 방어 가능.

---

## 8. 두 프로젝트 비교 요약

| 차원 | Gas Town (`steveyegge/gastown`) | Goosetown (`block/goosetown`) |
|------|--------------------------------|-------------------------------|
| **언어** | Go 1.25 | Python/Bash |
| **규모** | 300+ Go 파일, 엔터프라이즈급 | ~수십 파일, 미니멀 |
| **저장소** | Dolt SQL Server | 로컬 Git + YAML |
| **에이전트 수** | 20~30 병렬 | 3~10 병렬 |
| **조율 방식** | Hook 기반 + Mail + Beads | gtwall 브로드캐스트 |
| **계층** | 다중 감시 계층 (Mayor→Deacon→Boot→Witness) | 플랫 (Orchestrator→Delegates) |
| **Delegate 생성** | 가능 (계층적) | 불가 (플랫) |
| **머지 전략** | Bors 스타일 Refinery (batch-then-bisect + re-imagine) | 수동 |
| **세션 관리** | Seancing (전 세션 부활하여 질문) | 깨끗한 새 컨텍스트에서 요약 반환 |
| **상태 관리** | Dolt (6단계 생명주기) | gtwall 로그 + Beads (경량) |
| **모니터링** | Web Dashboard + Feed TUI + 플러그인 | 스팀펑크 Dashboard |
| **테마** | Mad Max 산업 도시 🏭 | 스팀펑크 거위 마을 🦆 |
| **대상** | 엔터프라이즈/팀 | 개인 개발자/리서치 |
| **연합** | Wasteland (구현 완료) | 해당 없음 |
| **비용** | $2,000~$5,000+/월 | 모델 비용만 (유연한 모델 선택) |
| **Wasteland 참여** | 네이티브 지원 | Dolt + DoltHub + Claude skill만으로 가능 |

---

## 9. 설계 패턴 및 교훈

### 9.1 미래 에이전트 시스템에 공통될 패턴 (Maggie Appleton 분석 기반)

1. **역할 기반 특화 + 계층적 감독** — 각 에이전트에 단일 역할을 부여하여 정밀한 프롬프팅, 권한 제한, 동시 실행 가능. 명확한 명령 체계.
2. **영속적 역할/작업, 임시 세션** — 중요한 정보(ID, 작업)를 Git에 저장하고 세션을 자유롭게 소멸/생성. Context rot 문제 해결.
3. **연속적 작업 공급** — 워커 에이전트가 절대 유휴 상태가 되지 않도록 작업 큐와 Hook을 유지. 감시자가 주기적 heartbeat로 정체 감지.
4. **에이전트 관리 머지 큐** — 전용 머지 에이전트가 충돌 해결, 필요시 재상상. 인간의 머지 부담 제거.
5. **Git-backed Persistence** — 세션 크래시에도 상태 유지. Beads가 이 패턴의 원시 단위.

### 9.2 핵심 교훈

1. **디자인이 새로운 병목** — 에이전트가 코드를 다 쓸 때, 무엇을 만들지/어떻게 만들지 결정하는 것이 가장 비싼 작업
2. **에이전트는 불안정하다** — 크래시, 정체, 환각이 기본 전제. 감시와 복구가 1급 시민
3. **조율 비용은 에이전트 수에 비례** — 플랫 계층과 최소 통신이 핵심
4. **검증은 신뢰를 대체** — Yearbook Rule: 자기 검증은 무가치. 다중 모델 교차 검증이 필수
5. **기록은 미래의 자산** — 결정, 근거, 발견사항, 미해결 질문 모두 기록. 평판은 작업 이력의 함수
6. **바이브 디자인의 위험** — 빠르게 만드는 것과 잘 설계하는 것은 다르다. Gas Town은 이 교훈의 살아있는 예시

---

## 10. 참고 링크

### 1차 소스
- [Steve Yegge - Welcome to the Wasteland: A Thousand Gas Towns (2026.03.04)](https://steve-yegge.medium.com/welcome-to-the-wasteland-a-thousand-gas-towns-a5eb9bc8dc1f)
- [Maggie Appleton - Gas Town's Agent Patterns, Design Bottlenecks, and Vibecoding at Scale](https://maggieappleton.com/gastown)
- [Block Goose Blog - Gas Town Explained: How to Use Goosetown (2026.02.19)](https://block.github.io/goose/blog/2026/02/19/gastown-explained-goosetown/)

### 레포지토리
- [GitHub - steveyegge/gastown](https://github.com/steveyegge/gastown)
- [GitHub - block/goosetown](https://github.com/block/goosetown)

### 커뮤니티
- [Hacker News Discussion](https://news.ycombinator.com/item?id=47250133)
- [gastownhall.ai](https://gastownhall.ai) — Wasteland 리더보드/캐릭터 시트
- Gas Town Discord 커뮤니티

### 관련 분석
- [Anthropic - Effective Harnesses for Long-Running Agents (2025.11)](https://www.anthropic.com/research/swe-bench-sonnet) — Beads와 유사한 외부 작업 추적 패턴 제안
- Cursor의 Graphite 인수 — Stacked Diffs 방식의 에이전트 워크플로우
