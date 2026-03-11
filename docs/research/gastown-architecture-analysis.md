# Gastown & Goosetown 아키텍처 분석 리서치

> 작성일: 2026-03-11
> 소스: Steve Yegge Medium 블로그, Block Goose 블로그, GitHub 레포지토리 분석

---

## 1. 개요: "Gastown"이란 무엇인가?

**Gastown**은 Steve Yegge가 제안한 **멀티 에이전트 오케스트레이션 패러다임**이다. AI 코딩 에이전트(주로 Claude Code) 20~30개를 병렬로 조율하여 대규모 소프트웨어 개발 작업을 수행하는 시스템을 말한다.

이 패러다임을 구현한 두 프로젝트가 존재한다:

| 프로젝트 | 저장소 | 성격 |
|---------|--------|------|
| **Gas Town** | `steveyegge/gastown` | 엔터프라이즈급 풀스택 구현 (Go) |
| **Goosetown** | `block/goosetown` | 미니멀 연구 중심 구현 (Python/Bash) |

---

## 2. 철학과 핵심 원칙

### 2.1 공통 철학

#### Research-First, Build-Second (조사 먼저, 구현 나중)
- 모든 비자명한 작업은 병렬 리서치로 시작한다
- "서류 위의 놀라움이 코드 위의 놀라움보다 저렴하다"
- 3개 소스에서 80% 신뢰도면 충분히 진행

#### Propulsion Principle (추진 원칙)
- 에이전트는 즉시 실행한다. 질문하지 않고, 기다리지 않고, 서론 없이 바로 작업
- "물리학이지, 예절이 아니다: 모든 지연의 순간은 시스템이 멈추는 순간"

#### Context is Finite (컨텍스트는 유한하다)
- 오케스트레이터는 자신의 컨텍스트를 사수해야 한다
- 방향 설정(오케스트레이터)과 작업 실행(위임자)을 엄격히 분리
- 산출물을 만드는 모든 작업은 위임한다

#### Write as You Go (가면서 기록하라)
- 워커와 라이터는 점진적으로 산출물을 만든다
- 매 도구 호출마다 디스크에 일관된 부분 산출물을 남긴다
- 취소된 에이전트의 8/10 섹션 > 메모리에만 있던 완전한 작업

### 2.2 Gas Town 고유 원칙

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

---

## 3. 아키텍처 비교

### 3.1 Gas Town (steveyegge/gastown) — 엔터프라이즈 아키텍처

```
┌─────────────────────────────────────────────────────────────┐
│                      TOWN (도시) 레벨                        │
│                                                             │
│  Mayor 🎩 ─── AI 코디네이터, 전체 워크스페이스 컨텍스트        │
│  Deacon ──── 데몬 감시견, 연속 순찰 사이클                    │
│  Boot ────── Deacon의 감시견 (5분마다 Deacon 생존 확인)        │
│  Dogs ────── 유지보수 에이전트 (압축, 건강 체크, 아카이브)       │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                      RIG (작업대) 레벨                        │
│                                                             │
│  Witness ──── Polecat 건강 모니터링, 정체된 워커 nudge         │
│  Refinery ─── 머지 큐 관리 (Bors 스타일 batch-then-bisect)    │
│  Polecats 🦨 ─ 워커 에이전트 (영속 ID, 임시 세션)              │
│  Crew ────── 인간 워크스페이스                                │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│                    인프라스트럭처 레벨                         │
│                                                             │
│  Dolt SQL Server (포트 3307) ─── 모든 Beads 데이터 저장       │
│  Git Worktrees ─── Polecat별 격리된 작업 공간                 │
│  Beads ─── Git 기반 원자적 작업 단위                          │
│  Mail ─── 영속적 메시징 시스템                                │
│  Tmux ─── 터미널 멀티플렉싱                                  │
│  Dashboard ─── 실시간 웹 UI (포트 8080)                      │
└─────────────────────────────────────────────────────────────┘
```

**기술 스택:** Go 1.25, Dolt (MySQL 프로토콜), Charmbracelet TUI, gRPC, Cobra CLI

**핵심 작업 흐름 (GUPP 원칙: "Hook에 작업이 있으면, 반드시 실행하라"):**
1. Mayor 또는 인간이 Convoy(작업 배치)를 생성
2. `gt sling <bead-id> <rig>` — 작업을 에이전트 Hook에 배정
3. 에이전트가 `gt hook`으로 Hook 감지
4. 즉시 실행 (대기 없음)
5. `gt done` — 완료 제출 후 idle 전환
6. Refinery가 머지 큐 처리
7. Witness가 좀비/정체 상태 모니터링

**Polecat 생명주기:**
- **Identity** (영구) — 에이전트 Bead, CV 체인, 작업 이력
- **Sandbox** (배정간 영속) — Git worktree
- **Session** (임시) — Claude 컨텍스트 윈도우
- 상태: `WORKING → (handoff cycles) → IDLE → (next sling) → WORKING`

**머지 큐 (Refinery) — Bors 스타일:**
1. Batch: 모든 MR을 main 위에 스택으로 rebase
2. Test: 스택 tip에서 테스트 실행
3. PASS → 전체 fast-forward merge
4. FAIL → 이진 탐색으로 범인 찾기

**데이터 생명주기:** `CREATE → LIVE → CLOSE → DECAY → COMPACT → FLATTEN`

### 3.2 Goosetown (block/goosetown) — 미니멀 연구 아키텍처

```
┌──────────────────────────────────────────────────────┐
│               Orchestrator (메인 세션)                 │
│     컨텍스트 관리, Flock 생성, 결과 합성               │
│                                                      │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐  │
│  │ Researchers  │ │   Workers    │ │  Reviewers   │  │
│  │  (3~6 병렬)  │ │  (병렬 실행)  │ │  (품질 게이트) │  │
│  │              │ │              │ │              │  │
│  │ · Local      │ │ · 코드 작성   │ │ · 보안       │  │
│  │ · GitHub     │ │ · 파일 생성   │ │ · 정확성     │  │
│  │ · Reddit     │ │ · 기능 구현   │ │ · 통합 검증   │  │
│  │ · StackOverflow│ │            │ │              │  │
│  │ · arXiv      │ │              │ │              │  │
│  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘  │
│         │                │                │          │
│         └────────────────┴────────────────┘          │
│                          │                           │
│                    ┌─────┴─────┐                     │
│                    │  gtwall   │                     │
│                    │ (Town Wall)│                     │
│                    │ 브로드캐스트 │                     │
│                    └───────────┘                     │
│                                                      │
│  + Writers (합성 전문가) — 리서치 → 문서 변환            │
└──────────────────────────────────────────────────────┘
```

**기술 스택:** Python 3.11+, Bash, FastAPI/Starlette, lit-html, Goose CLI

**핵심 구조적 차이점:**
- **플랫 계층** — 위임자(Delegate)가 다른 위임자를 생성할 수 없음
- **gtwall** — 유일한 에이전트간 통신 채널 (POSIX 텍스트 로그 + 위치 추적)
- **방송 조율, 공유 상태 없음** — 에이전트는 메모리나 도구 출력을 공유하지 않음

**워크플로우 5단계:**
1. **Research** — 3~6 병렬 리서처가 다양한 소스 탐색, gtwall에 발견사항 게시
2. **Process** — 결과 통합, 갭 식별, 필요시 후속 리서치 생성
3. **Plan & Dispatch** — 리서치 합성 → 계획, 워커/라이터 배정 (분리된 파일)
4. **Review** — 다차원 품질 리뷰 (crossfire: 다중 모델 적대적 QA)
5. **Synthesize** — 모든 조각을 통합, 리뷰어 발견사항 반영, 충돌 해결

---

## 4. 핵심 컴포넌트 상세

### 4.1 Beads — 작업 추적의 원자 단위

두 프로젝트 모두 **Beads**를 작업 추적 원시 단위로 사용한다:

| 속성 | Gas Town | Goosetown |
|------|----------|-----------|
| 저장소 | Dolt SQL Server | 로컬 Git 기반 |
| 스키마 | v6 (issues, dependencies, labels, comments, events, interactions, metadata) | 경량 이슈 트래커 |
| 레벨 | Town-level (`hq-*`) + Rig-level (`gt-*`, `bd-*`) | 단일 레벨 |
| 용도 | 전체 작업 생명주기 관리 | 크래시 복구, 진행 추적 |

### 4.2 gtwall / Town Wall — 에이전트간 통신

**Goosetown의 gtwall** (~400줄 Bash):
- 불변 로그 + 리더별 위치 추적
- 원자적 쓰기 (lock directory, stale lock 감지)
- 포맷: `timestamp|sender_id|message` (파이프 이스케이프)
- 리서처는 3~5 도구 호출마다 발견사항 게시

**Gas Town의 Mail 시스템:**
- `issue_type='message'` Beads + 의존성 기반 스레딩
- 세션 재시작 간 영속
- `gt mail send`, `gt mail inbox`, `gt mail read`

### 4.3 Dashboard — 실시간 모니터링

**Gas Town Dashboard:**
- HTTP 서버 (포트 8080), htmx 자동 새로고침
- 에이전트, 콘보이, 훅, 머지 큐, 에스컬레이션 표시
- 커맨드 팔레트로 브라우저에서 gt 명령 실행

**Goosetown Dashboard:**
- FastAPI/Starlette (localhost:4242~4300)
- 스팀펑크 테마 마을 시각화
- Village (에이전트 위치), Registry (위임자 목록), Bulletin (벽 메시지),
  Workshop (상세 뷰), Clockworks (통계), Editor (파일 브라우저)

---

## 5. "The Wasteland" — 연합(Federation) 비전

Steve Yegge의 2026년 3월 블로그 "Welcome to the Wasteland: A Thousand Gas Towns"은 Gastown의 다음 진화 단계를 제시한다:

### 핵심 컨셉
- **100x 토큰 지출 확장**: AI 도구의 모든 폼팩터 혁신은 100배의 토큰 지출 증가를 수반한다. Gas Town을 100배 확장하는 방법은? 수백 명의 Gas Town 사용자를 **연합(federation)**시켜 함께 구축하는 것
- **공유 Wanted Board**: 사람들이 아이디어를 올리고, 다른 사람들의 Gas Town이 그 아이디어 구현을 돕는 대규모 공유 작업 게시판
- **Stamps & Passbooks**: PR이 수락되면 기여자의 Passbook에 도장을 찍음. 기여자는 평판을 얻고, 영구 원장에 기록되어 **이동 가능한 이력서** 역할
- **RPG화**: 스탬프, 리더보드, 캐릭터 시트 등 — 메타모포시스가 RPG로 불가항력적으로 전환 중
- **실적**: 2개월 만에 2,400 제출 PR, 1,500 병합, 450명 이상의 고유 기여자

### 핵심 인물
- **Julian Knutsen** (ex-CashApp/Block/Bitcoin) — Wasteland 구현 설계
- **Dr. Matt Beane** (*The Skill Code* 저자) — 스킬 및 멘토링 시스템 담당
- **Chris Sells** — gastownhall.ai 및 Discord 커뮤니티 운영
- **Tim Sehn** (DoltHub 창업자/CEO) — 기능 및 버그 수정 지원

---

## 6. 두 프로젝트 비교 요약

| 차원 | Gas Town (`steveyegge/gastown`) | Goosetown (`block/goosetown`) |
|------|--------------------------------|-------------------------------|
| **언어** | Go 1.25 | Python/Bash |
| **규모** | 300+ Go 파일, 엔터프라이즈급 | ~수십 파일, 미니멀 |
| **저장소** | Dolt SQL Server | 로컬 Git + YAML 프론트매터 |
| **에이전트 수** | 20~30 병렬 | 3~10 병렬 |
| **조율 방식** | Hook 기반 + Mail + Beads | gtwall 브로드캐스트 |
| **계층** | 다중 감시 계층 (Mayor→Deacon→Boot→Witness) | 플랫 (Orchestrator→Delegates) |
| **머지 전략** | Bors 스타일 Refinery (batch-then-bisect) | 수동 |
| **상태 관리** | Dolt (6단계 생명주기) | gtwall 로그 + Beads (경량) |
| **모니터링** | Web Dashboard + Feed TUI + 플러그인 | 스팀펑크 Dashboard |
| **테마** | Mad Max 스타일 산업 도시 | 스팀펑크 거위 마을 |
| **대상** | 엔터프라이즈/팀 | 개인 개발자/리서치 |
| **연합** | Wasteland (계획/구현 중) | 해당 없음 |

---

## 7. 설계 패턴 및 교훈

### 7.1 공통 설계 패턴

1. **Hierarchical Orchestration** — 단일 오케스트레이터가 다수의 특화 에이전트 조율
2. **Broadcast Communication** — 공유 상태 대신 브로드캐스트 채널 사용
3. **Git-backed Persistence** — 세션 크래시에도 상태 유지
4. **Role-based Specialization** — 에이전트별 역할 분리 (리서처, 워커, 리뷰어, 라이터)
5. **Incremental Artifact Production** — 점진적 산출물 생성으로 부분 실패 시에도 가치 보존

### 7.2 핵심 교훈

1. **컨텍스트 윈도우는 진짜 제약** — 상태를 외부에 저장하고, 에이전트 컨텍스트를 사수해야 한다
2. **에이전트는 불안정하다** — 크래시, 정체, 오류가 기본 전제. 감시와 복구가 필수
3. **조율 비용은 에이전트 수에 비례** — 플랫 계층과 최소 통신이 핵심
4. **검증은 신뢰를 대체** — 다중 모델 교차 검증(crossfire)이 단일 모델 자기 검증보다 우월
5. **기록은 미래의 자산** — 결정, 근거, 발견사항, 미해결 질문 모두 기록

---

## 8. 참고 링크

- [Steve Yegge - Welcome to the Wasteland: A Thousand Gas Towns](https://steve-yegge.medium.com/welcome-to-the-wasteland-a-thousand-gas-towns-a5eb9bc8dc1f)
- [Block Goose Blog - Gas Town Explained: Goosetown](https://block.github.io/goose/blog/2026/02/19/gastown-explained-goosetown/)
- [GitHub - steveyegge/gastown](https://github.com/steveyegge/gastown)
- [GitHub - block/goosetown](https://github.com/block/goosetown)
- [Hacker News Discussion](https://news.ycombinator.com/item?id=47250133)
- [GasTown and the Two Kinds of Multi-Agent](https://paddo.dev/blog/gastown-two-kinds-of-multi-agent/)
