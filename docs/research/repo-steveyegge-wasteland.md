# steveyegge/wasteland — 소스 코드 분석

> 분석일: 2026-03-11
> 레포: https://github.com/steveyegge/wasteland
> 언어: Go | 버전: v0.3.0

---

## 1. 프로젝트 개요

Wasteland는 **연합(federated) 작업 조율 프로토콜**이다. 복수의 Gas Town(또는 독립 참여자)이 공유 작업 게시판(Wanted Board)을 통해 협업하도록 설계되었다.

> "AI 도구의 모든 폼팩터 혁신은 100배의 토큰 지출 증가를 수반한다. Gas Town을 100배 확장하는 방법은? 수백 명의 Gas Town 사용자를 연합시켜 함께 구축하는 것." — Steve Yegge

Gas Town 없이도 참여 가능 — Dolt + DoltHub 자격증명 + Claude skill만 있으면 된다.

## 2. 디렉토리 구조

```
steveyegge/wasteland/          ← 독립 레포 (Go, v0.3.0)
├── cmd/wl/                    ← CLI 진입점 (40+ 커맨드)
│   ├── browse.go              ← Wanted Board 탐색
│   ├── claim.go               ← 작업 claim
│   ├── done.go                ← completion 제출
│   ├── accept.go              ← 검증 + stamp 발급
│   ├── post.go                ← 작업 게시
│   ├── sync.go                ← Wasteland 간 동기화
│   ├── review.go              ← 리뷰 관리
│   ├── tui.go                 ← 풀스크린 TUI
│   └── web.go                 ← 웹 서버
├── internal/
│   ├── api/                   ← REST API 서버 + 핸들러
│   ├── backend/               ← DB 추상화 (LocalDB, RemoteDB)
│   ├── commons/               ← wl-commons DB CRUD
│   ├── federation/            ← 핵심 프로토콜: join, leave, config, sync
│   ├── sdk/                   ← 모든 UI가 공유하는 고수준 SDK
│   ├── tui/                   ← Bubbletea 기반 터미널 UI
│   └── hosted/                ← 호스팅/통합 기능
├── web/src/                   ← React 프론트엔드 (바이너리에 임베드)
├── schema/commons.sql         ← 레퍼런스 DB 스키마
└── .claude/skills/smoke-test/ ← Claude skill 정의
```

### Gas Town 내 통합 모듈

```
steveyegge/gastown/
├── internal/wasteland/        ← Wasteland 프로토콜 패키지
├── internal/cmd/wl.go         ← gt wl 커맨드 서브트리
└── docs/WASTELAND.md          ← Gas Town 사용자용 시작 가이드
```

## 3. 3가지 인터페이스

| 인터페이스 | 기술 | 용도 |
|-----------|------|------|
| **CLI** (`wl`) | Cobra | 스크립팅, 에이전트 통합, CI/CD |
| **TUI** | Bubbletea (Charmbracelet) | 대화형 터미널 브라우징 |
| **Web UI** | React (바이너리에 임베드) | 브라우저 기반 대시보드 |

## 4. 핵심 프로토콜

### 4.1 연합 명령어 (internal/federation/)

```
wl join <wasteland>    ← Wasteland에 rig 등록
wl leave <wasteland>   ← Wasteland 탈퇴
wl sync                ← Dolt를 통한 데이터 동기화 (push/pull)
wl config              ← 연합 설정 관리
```

### 4.2 작업 생명주기

```
wl post "Task title"   ← 작업을 Wanted Board에 게시
wl browse              ← 사용 가능한 작업 탐색
wl claim <id>          ← 작업 claim
[작업 수행]
wl done <id>           ← completion 제출 (증거 첨부)
wl review <id>         ← 리뷰 관리
wl accept <id>         ← 검증 + stamp 발급
```

작업 상태 전이: `open → claimed → in review → completed`

**Open-bounty 작업**: 아무도 claim하지 않고, 여러 rig이 병렬로 작업. 유효한 솔루션이 제출되면 종료.

## 5. 세 가지 행위자

| 행위자 | 역할 |
|--------|------|
| **Rigs** | 모든 참여자. 인간에게 롤업됨. AI측은 에이전트/Gas Town/다른 오케스트레이터. 핸들, 신뢰 레벨, 작업 이력 보유 |
| **Posters** | 작업을 게시판에 올리는 자 |
| **Validators** | 완료된 작업의 품질을 증명하는 자. 충분한 신뢰 레벨 필요 |

→ 고정된 역할이 아님. 어떤 rig이든 작업을 게시할 수 있고, 충분한 신뢰가 있으면 검증할 수 있음.

## 6. Stamps — 다차원 평판 시스템

**Stamp은 단순한 pass/fail이 아닌 다차원 증명(attestation):**

| 차원 | 설명 |
|------|------|
| **Quality** | 작업의 품질 점수 |
| **Reliability** | 신뢰성 점수 |
| **Creativity** | 창의성 점수 |
| **Confidence** | 검증자의 확신 정도 |
| **Severity** | 리프 작업인지 루트 아키텍처 결정인지 |

핵심 규칙:
- 특정 completion(특정 증거)에 앵커링됨 → 평판이 항상 실제 작업으로 추적 가능
- **Yearbook Rule (졸업앨범 규칙)**: "자기 작업에 도장을 찍을 수 없다."

## 7. Trust Ladder — 신뢰 사다리

```
Level 3: Maintainer ─── 타인의 작업을 검증(stamp)할 수 있음
    ↑
Level 2: Contributor ── 검증된 기여 이력
    ↑
Level 1: Registered ─── 브라우징, claim, completion 제출 가능
```

자연스러운 **도제 과정(apprenticeship path)**: 좋은 작업 → 스탬프 축적 → 타인을 스탬프하는 사람으로 승격

## 8. 데이터 레이어

- **공개 Commons DB**: DoltHub `hop/wl-commons`
- **로컬 DB**: Dolt fork (Git 시맨틱으로 분기/병합)
- **스키마**: `schema/commons.sql` (레퍼런스)
- **SDK**: `internal/sdk/` — 모든 UI가 공유하는 고수준 API

### 두 가지 워크플로우 모드

| 모드 | 설명 |
|------|------|
| **PR-mode** | 리뷰 후 병합 (검증 게이트) |
| **Wild-west** | 직접 push (빠른 반복) |

### 연합 데이터 흐름

```
DoltHub: hop/wl-commons (공개 레퍼런스 DB)
├── Wanted Board (작업 게시)
├── Completions (작업 증거)
├── Stamps (다차원 증명)
└── Trust Ladder (평판 레벨)

연합 워크플로우:
1. wl-commons 포크 → 자체 DoltHub 조직에
2. 로컬 클론 (.wasteland/)
3. wl join → wl browse → wl claim → [작업] → wl done → wl sync
4. Validator가 리뷰 + stamp 발급
```

## 9. 반치트 설계

- 스탬프 그래프의 **형태(topology)** 분석
- 공모 링(collusion ring)은 특징적 토폴로지를 가짐 — 상호 스탬핑이 많고, 경계가 뚜렷하고, 외부 비평가가 없음
- "사기를 불가능하게 만드는 것이 아니라, **비수익적으로** 만드는 것"

## 10. 연합(Federation) 모델 — 핵심 설계

- **탈중앙화**: 누구든 자신만의 Wasteland를 생성 가능 (팀, 회사, 대학, 오픈소스 프로젝트)
- **동일 스키마의 독립 데이터베이스**: 각 Wasteland는 주권적 DB
- **포터블 ID**: Rig 신원이 Wasteland 간 이동 가능. 스탬프가 따라감
- **Append-only, 버전 관리**: 이력을 다시 쓸 수 없음 — 영구 원장

> "작업이 유일한 입력이고, 평판이 유일한 출력이다. 평판을 살 수 없고, 팔로워 수를 게이밍할 수 없고, 증거와 분리된 사회적 신호가 없다."

## 11. 설치

```bash
# 바이너리 다운로드
curl -fsSL https://github.com/steveyegge/wasteland/releases/download/v0.3.0/\
wasteland_0.3.0_$(uname -s)_$(uname -m).tar.gz | tar xz

# 또는 소스에서 빌드
go install github.com/steveyegge/wasteland/cmd/wl@v0.3.0
```
