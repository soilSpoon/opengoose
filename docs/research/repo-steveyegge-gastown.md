# steveyegge/gastown — 소스 코드 분석

> 분석일: 2026-03-11
> 레포: https://github.com/steveyegge/gastown
> 언어: Go 1.25 | 라이선스: MIT | 규모: 300+ Go 파일

---

## 1. 프로젝트 개요

Gas Town은 **AI 코딩 에이전트 20~30개를 병렬 조율하는 멀티 에이전트 오케스트레이션 시스템**이다. Steve Yegge가 2026년 1월 1일에 발표했으며, 17일/75k LOC/2000 커밋으로 "100% 바이브코딩"되었다.

> "Gas Town은 복잡하다. 내가 원해서가 아니라, 자급자족하는 기계가 될 때까지 계속 컴포넌트를 추가해야 했기 때문이다."

## 2. 디렉토리 구조

```
steveyegge/gastown/
├── cmd/gt/main.go              ← 바이너리 진입점 → cmd.Execute()
├── internal/
│   ├── cmd/                    ← 300+ 커맨드 구현 (Cobra CLI)
│   │   ├── root.go             ← CLI 루트, prerun 훅, beads 버전 체크
│   │   ├── sling.go            ← 작업 배정 (auto-convoy, polecat 자동 스폰)
│   │   ├── done.go             ← 완료 제출 (머지 큐 + Witness 에스컬레이션)
│   │   ├── convoy.go           ← 배치 작업 관리 (create, list, land)
│   │   ├── mayor.go            ← Mayor 세션 관리
│   │   ├── mail.go             ← 비동기 통신 (queue/interrupt 모드)
│   │   └── wl.go               ← Wasteland CLI 래퍼 → internal/wasteland/
│   ├── polecat/
│   │   ├── types.go            ← 상태 머신: Working→Idle→Stuck→Zombie→Done
│   │   └── manager.go          ← 스폰 시퀀스, 네임풀, worktree 관리
│   ├── daemon/daemon.go        ← 중앙 복구 루프, 대량 사망 감지
│   ├── convoy/                 ← 상태 전이: open↔closed, staged_*→open/closed
│   ├── events/events.go        ← JSONL 이벤트 (sling/hook/done/mail/spawn/kill...)
│   ├── mail/types.go           ← 메시지: priority(low~urgent), type(task/scavenge/notification/reply)
│   ├── beads/beads.go          ← bd CLI 추상화, external:prefix:id 크로스 rig 참조
│   ├── mayor/                  ← 글로벌 코디네이터
│   ├── witness/                ← Polecat 건강 순찰
│   ├── deacon/                 ← 백그라운드 작업 실행
│   ├── dog/                    ← Deacon의 헬퍼 워커
│   ├── refinery/               ← Bors 스타일 머지 큐
│   ├── session/                ← 세션 생명주기
│   ├── config/types.go         ← 설정 계층: Town→Rig→Agent
│   ├── wasteland/              ← Wasteland 프로토콜 통합
│   ├── web/                    ← 대시보드 HTTP 서버
│   ├── feed/                   ← 활동 피드 TUI
│   ├── tmux/                   ← Tmux 세션 관리
│   └── [40+ 더 많은 패키지]
├── plugins/                    ← Dog 플러그인 (compactor, archive, quality-review, stuck-agent 등)
├── templates/                  ← 역할 프롬프트 템플릿 (런타임 CLAUDE.md 주입)
├── docs/                       ← 설계 문서
└── go.mod                      ← 의존성: Beads, Dolt, Cobra, Charmbracelet TUI
```

## 3. 에이전트 역할 계층

```
       [You / Human]
            │
        [Mayor] ─────── 절대 코드를 쓰지 않음. 지시와 조율만.
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

| 역할 | 위치 | 기능 |
|------|------|------|
| **Mayor** | `internal/mayor/` | 인간 컨시어지. 작업 분해, Polecat 배정, 절대 코드 작성 안 함 |
| **Polecat** | `internal/polecat/` | 임시 그런트 워커. 단일 작업 후 idle. 영속 ID + 임시 세션 |
| **Witness** | `internal/witness/` | Polecat 건강 순찰, 정체 감지 및 nudge |
| **Deacon** | `internal/deacon/` | 백그라운드 작업 실행 |
| **Boot** | - | Deacon 감시견 (5분마다 Deacon 생존 확인) |
| **Dogs** | `internal/dog/`, `plugins/` | 유지보수 에이전트 (압축, 건강 체크, 아카이브) |
| **Refinery** | `internal/refinery/` | Bors 스타일 머지 큐 (batch-then-bisect) |
| **Crew** | - | 인간 워크스페이스 |

## 4. 핵심 데이터 구조 (코드에서 추출)

```go
// Polecat 상태 머신 (polecat/types.go)
type Polecat struct {
    Name      string    // 고유 식별자 (네임풀에서 생성)
    Rig       string    // 부모 rig
    State     State     // Working|Idle|Stuck|Zombie|Done
    ClonePath string    // Git worktree 경로
    Branch    string    // Git 브랜치
    Issue     string    // 현재 배정된 작업
}

// Convoy 상태 전이 (convoy/)
// open ↔ closed
// staged_* → open/closed
// staged_* ↔ staged_*
// (open/closed → staged_* 불가)

// Mail 메시지 (mail/types.go)
type Message struct {
    ID, From, To, Subject, Body string
    Priority string  // low, normal, high, urgent
    Type     string  // task, scavenge, notification, reply
    Delivery string  // queue (주기적 체크) | interrupt (세션에 즉시 주입)
    ThreadID, ReplyTo string
}

// 이벤트 로깅 (events/events.go)
type Event struct {
    Timestamp  time.Time
    Source     string
    Type       string  // sling, hook, unhook, handoff, done, mail, spawn, kill, nudge...
    Actor      string
    Payload    map[string]interface{}
    Visibility string  // "audit" | "feed" | "both"
}
```

## 5. 핵심 작업 흐름

### GUPP 원칙: "Hook에 작업이 있으면, 반드시 실행하라"

```
gt sling <bead-id> <rig>    → 작업을 에이전트 Hook에 배정
                               ↓
    Polecat이 gt hook으로 감지 → 즉시 실행 (대기 없음)
                               ↓
    gt done                  → 완료 제출 후 idle 전환
                               ↓
    Refinery가 머지 큐 처리
    Witness가 좀비/정체 상태 모니터링
```

### Polecat 스폰 시퀀스 (manager.go)
1. 네임풀에서 고유 이름 생성 (테마 설정 가능)
2. 레퍼런스 clone에서 Git worktree 생성
3. Beads DB 초기화 (Dolt 재시도: 10회, 최대 30초, ±25% 지터)
4. Tmux 세션 생성 + 시작 비콘 (환경변수로 구조화 정보 주입)
5. 준비 신호 대기 후 작업 배정

### Daemon 복구 루프 (daemon.go)
```
무한 루프:
  ├── Polecat 심장박동 파일 모니터링
  ├── "대량 사망" 감지 (30초 내 3+ 세션 다운)
  ├── Witness/Refinery 실패 시 자동 재시작
  ├── Git pull 재시도 (에스컬레이팅 로그)
  ├── 예약 유지보수 (doctor molecule pouring)
  ├── JSONL 백업 → git (feed curator)
  └── 재시작 루프에 지수 백오프
관리 대상: ConvoyManager, DoltServerManager, KRCPruner, feed.Curator
```

### 머지 큐 (Refinery) — Bors 스타일
1. **Batch**: 모든 MR을 main 위에 스택으로 rebase
2. **Test**: 스택 tip에서 테스트 실행
3. **PASS** → 전체 fast-forward merge
4. **FAIL** → 이진 탐색으로 범인 찾기
5. 충돌 심할 경우 **"재상상(re-imagine)"** — 원래 의도 유지하며 새 코드베이스에 맞게 재작성

## 6. 인프라

| 컴포넌트 | 기술 | 역할 |
|----------|------|------|
| **Dolt SQL Server** | 포트 3307, MySQL 프로토콜 | 모든 Beads 데이터 저장 |
| **Git Worktrees** | Polecat별 격리 | 에이전트간 파일 충돌 방지 |
| **Beads** | `internal/beads/beads.go` | bd CLI 래퍼, 재시도 로직, 크로스 rig 참조 |
| **Mail** | `internal/mail/` | 비동기 메시징 (queue/interrupt) |
| **Tmux** | `internal/tmux/` | 세션 멀티플렉싱 |
| **Dashboard** | `internal/web/` | 포트 8080, htmx 자동 새로고침 |
| **OTEL** | best-effort | 실패해도 커맨드 블로킹 안 함 |

## 7. 주요 코드 패턴

| 패턴 | 위치 | 목적 |
|------|------|------|
| Git 기반 원자적 상태 | beads/, hooks/ | ACID 시맨틱 via git |
| 지수 백오프 + 지터 | polecat/manager.go | thundering herd 방지 |
| 파일 락 (flock) | events/, status | 크로스 프로세스 동기화 |
| 역할 인식 기본값 | config/types.go | crew vs polecat vs autonomous 별 다른 동작 |
| 세션 비콘 | session/ | 환경변수를 통한 구조화된 시작 정보 주입 |
| Worktree 보존 | polecat/ | 완료된 polecat의 worktree를 재사용 (셋업 지연 감소) |
| Seancing (강신술) | session/ | 새 세션이 이전 세션을 부활시켜 미완료 작업에 대해 질문 |

## 8. 설정 계층

```
Town 레벨 (settings/config.json)
├── TownSettings: CLI 테마, 기본 에이전트, 역할→에이전트 매핑
├── MayorConfig: 테마, 데몬, deacon 설정
└── TownConfig: 아이덴티티 (type, version, name, owner)

Rig 레벨 (.beads/config.json)
└── BeadsConfig: 이슈 프리픽스, 기본 브랜치, 커스텀 타입

Runtime (RuntimeConfig)
├── Hook 프로바이더 설정 (Claude, Gemini 등)
├── 에이전트 프리셋 (세션 ID 환경변수)
└── 운영 임계값 (타임아웃, 재시도)
```

## 9. 플러그인 시스템

`plugins/` 디렉토리의 Dog 플러그인:
- `compactor-dog` — Dolt 커밋 성장 모니터링
- `dolt-archive` — 스크럽된 스냅샷 내보내기
- `quality-review` — 코드 품질 검사
- `session-hygiene` — 세션 정리
- `git-hygiene` — Git 유지보수
- `github-sheriff` — GitHub 통합
- `stuck-agent-dog` — 정체 에이전트 감지
- `rebuild-gt` — 바이너리 재빌드

## 10. 데이터 생명주기

`CREATE → LIVE → CLOSE → DECAY → COMPACT → FLATTEN`

Two-level Beads:
- **Town-level** (`~/.gt/.beads/`) — `hq-*` 프리픽스, 크로스 rig 조율 (Mayor, Deacon, convoys)
- **Rig-level** (`<rig>/mayor/rig/.beads/`) — `gt-*`, `bd-*` 프리픽스, 프로젝트 작업
