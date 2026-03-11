# TinyAGI/tinyclaw — 프로젝트 분석

> 분석일: 2026-03-11
> 레포: https://github.com/TinyAGI/tinyclaw
> 언어: Node.js (Bash + TypeScript) | 라이선스: MIT | GitHub 스타: ~3k

---

## 1. 프로젝트 개요

TinyClaw는 **멀티 에이전트, 멀티 팀, 멀티 채널 AI 어시스턴트 오케스트레이터**다. Claude Code나 OpenAI Codex 같은 CLI 에이전트를 여러 개 동시에 실행하면서, Discord/Telegram/WhatsApp/웹 등 다양한 채널로 접근할 수 있게 해준다.

> "TinyClaw는 에이전트 오케스트레이터로 설계된 게 아니다. 서로 협업하는 개인 에이전트 팀이다." — @jianxliao

Gas Town이 "20~30개 에이전트의 자율 공장"을 지향한다면, TinyClaw는 **"항상 켜져 있는 개인 AI 비서 팀"**에 가깝다.

---

## 2. 아키텍처

```
┌────────────────────────────────────────────────────────────┐
│                      입력 채널                               │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │
│  │ Discord  │ │ Telegram │ │ WhatsApp │ │ Web (Office) │  │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └──────┬───────┘  │
│       └─────────────┴────────────┴──────────────┘          │
│                          │                                  │
├──────────────────────────▼──────────────────────────────────┤
│              SQLite Queue (WAL mode)                        │
│              ~/.tinyclaw/tinyclaw.db                        │
│              pending → processing → completed/dead          │
│              재시도 5회, dead-letter queue                    │
├──────────────────────────┬──────────────────────────────────┤
│              Queue Processor (병렬 분배)                     │
│              에이전트별 순차, 에이전트간 병렬                   │
├──────────┬───────────────┼───────────────┬──────────────────┤
│ Agent A  │    Agent B    │    Agent C    │    Agent D       │
│ (coder)  │   (writer)    │  (reviewer)   │   (devops)       │
│ ~/ws/a/  │   ~/ws/b/     │   ~/ws/c/     │   ~/ws/d/        │
│ 격리된    │   격리된       │   격리된       │   격리된          │
│ worktree │   worktree    │   worktree    │   worktree       │
├──────────┴───────────────┴───────────────┴──────────────────┤
│              팀 채팅룸 (비동기 브로드캐스트)                    │
│              [#team_id: message] 태그 포맷                   │
└────────────────────────────────────────────────────────────┘
```

### 핵심 설계 원칙

| 원칙 | 구현 |
|------|------|
| **에이전트 격리** | 에이전트별 전용 workspace 디렉토리, 독립 대화 이력, 개별 리셋 |
| **메시지 신뢰성** | SQLite WAL 모드, 원자적 트랜잭션, 5회 재시도 + dead-letter |
| **에이전트별 순차** | 한 에이전트의 메시지는 순서 보장, 다른 에이전트는 병렬 |
| **24/7 운영** | tmux 기반 데몬, 항상 켜져 있음 |
| **접근 제어** | Sender Pairing — 승인 코드 기반 화이트리스트 |

---

## 3. 에이전트 시스템

### 3.1 에이전트 설정

```json
{
  "agents": {
    "coder": {
      "name": "Code Assistant",
      "provider": "anthropic",
      "model": "sonnet",
      "working_directory": "~/tinyclaw-workspace/coder/"
    },
    "writer": {
      "name": "Tech Writer",
      "provider": "openai",
      "model": "gpt-4o",
      "working_directory": "~/tinyclaw-workspace/writer/"
    }
  }
}
```

각 에이전트는:
- 전용 workspace 디렉토리 (`~/tinyclaw-workspace/{agent_id}/`)
- 독립된 `.claude/` 설정
- 별도의 대화 이력 (CLI가 관리)
- heartbeat 모니터링
- 개별 리셋 가능

### 3.2 프로바이더 지원

| 프로바이더 | 하네스 | 비고 |
|-----------|--------|------|
| **Anthropic** | claude | Claude Code CLI 호출 |
| **OpenAI** | codex | Codex CLI 호출 |
| **커스텀** | claude/codex | OpenAI/Anthropic 호환 엔드포인트 |

```bash
# 커스텀 프로바이더 등록
tinyclaw provider add
# 에이전트에 할당
tinyclaw agent provider coder custom:my-proxy --model gpt-4o
```

인증 토큰을 프로바이더별로 저장하여, 별도 CLI 인증 없이 자동 주입.

### 3.3 메시지 라우팅

```
일반 메시지        → 기본 에이전트로 라우팅
@coder 버그 고쳐줘  → coder 에이전트로 직접 라우팅
@devteam 배포해줘   → devteam의 리더 에이전트로 라우팅
/reset             → 현재 에이전트 대화 초기화
/agent             → 사용 가능한 에이전트 목록
```

---

## 4. 팀 시스템

### 4.1 구조

```
Team "devteam"
├── Leader: coder (라우팅 대상)
├── Member: reviewer
├── Member: writer
└── Chatroom: #devteam (영속적 비동기 채팅)
```

### 4.2 협업 패턴

**Chain Execution (순차 핸드오프):**
```
coder가 코드 작성 → reviewer에게 핸드오프 → writer에게 핸드오프
```

**Fan-out (병렬 분배):**
```
리더가 작업 수신 → coder, reviewer, writer에게 동시 배포
```

### 4.3 팀 채팅룸

- 모든 팀에 영속적 채팅룸이 존재
- 에이전트가 `[#team_id: message]` 태그로 메시지 게시
- 팀원 전체에게 브로드캐스트
- API: `GET/POST /api/chatroom/:teamId`
- CLI에서 실시간 뷰어: `tinyclaw chatroom <team>`

---

## 5. TinyOffice — 웹 포탈

### 5.1 개요

TinyOffice는 TinyClaw의 **Next.js 기반 웹 대시보드**다. 핵심 포인트: **UI 전용** — 데몬 프로세스를 대체하지 않으며, 실행 중인 TinyClaw 백엔드에 연결하여 모니터링/조작한다.

```
tinyoffice/
├── public/              ← 정적 애셋
├── src/                 ← 소스 코드
├── components.json      ← UI 컴포넌트 설정
├── next.config.ts
├── package.json
├── postcss.config.mjs
└── tsconfig.json
```

### 5.2 8개 기능 영역

| 기능 | 설명 |
|------|------|
| **Dashboard** | 실시간 시스템 개요 — 에이전트 상태, 팀, 큐, 이벤트 피드 |
| **Chat Console** | 웹에서 에이전트와 대화. `@agent`, `@team` 라우팅 지원 |
| **Agent Management** | 에이전트 생성/편집/삭제, 프로바이더/모델 변경 |
| **Team Management** | 팀 생성, 멤버 추가/제거, 리더 지정 |
| **Tasks (Kanban)** | 칸반 보드 — 드래그 앤 드롭으로 스테이지 이동, 에이전트/팀 할당 |
| **Logs & Events** | 큐 상태 검사, 실시간 이벤트 스트리밍 (SSE) |
| **Settings** | `.tinyclaw/settings.json` 설정 편집기 |
| **Office View** | 에이전트 상호작용의 시각적 시뮬레이션 |

### 5.3 API 연동 (18개 엔드포인트)

| 카테고리 | 엔드포인트 | 메서드 |
|----------|-----------|--------|
| **메시지** | `/api/message` | POST |
| **에이전트** | `/api/agents/:id` | GET/PUT/DELETE |
| **팀** | `/api/teams/:id` | GET/PUT/DELETE |
| **작업** | `/api/tasks` | GET/POST/PUT/DELETE |
| **설정** | `/api/settings` | GET/PUT |
| **큐** | `/api/queue/status` | GET |
| **응답** | `/api/responses` | GET |
| **로그** | `/api/logs` | GET |
| **이벤트** | `/api/events/stream` | SSE |
| **채팅룸** | `/api/chatroom/:teamId` | GET/POST |

### 5.4 Office View

Goosetown의 Village Map처럼, TinyOffice에도 에이전트 상호작용을 시각적으로 보여주는 **Office View**가 있다.

> "TinyClaws가 이제 TinyOffice에서 자기 자리를 갖게 됐다 — Office View: 에이전트 상호작용의 시각적 시뮬레이션" — @jianxliao

Goosetown의 Village Map이 "거위가 건물로 걸어가는 스팀펑크 마을"이라면, TinyOffice의 Office View는 "에이전트가 책상에 앉아 있는 오피스 시뮬레이션"에 가깝다. 둘 다 실제 에이전트 상태 데이터를 시각화하지만, 추가 정보를 제공하진 않는다.

### 5.5 실행

```bash
cd tinyoffice
npm install
# .env.local에서 API URL 커스터마이징 (선택)
# NEXT_PUBLIC_API_URL=http://localhost:3777
npm run dev     # 개발 모드: localhost:3000
npm run build && npm run start  # 프로덕션
```

---

## 6. 메시지 큐 시스템

```
메시지 생명주기:

    채널 수신
        │
        ▼
    pending ──── 큐에 적재 (SQLite WAL, 원자적)
        │
        ▼
    processing ── 에이전트 CLI 호출 중
        │
    ┌───┴───┐
    ▼       ▼
completed  failed ── 재시도 (최대 5회)
                        │
                        ▼ (5회 초과)
                    dead-letter ── 수동 확인 필요
```

**핵심 보장:**
- WAL 모드로 읽기/쓰기 동시성 확보
- 에이전트별 순차 처리 (대화 컨텍스트 보존)
- 중복 방지
- 응답 추적: pending/acknowledged 상태

---

## 7. Sender Pairing (접근 제어)

```
미확인 발신자의 첫 메시지
    ↓
TinyClaw가 승인 코드 생성
    ↓
관리자가 tinyclaw pairing approve <code> 실행
    ↓
발신자 화이트리스트 등록
    ↓
이후 메시지 정상 처리

# 미승인 상태에서 추가 메시지 → 무시 (silent block)
```

---

## 8. CLI 명령 체계

| 카테고리 | 주요 명령 |
|----------|----------|
| **코어** | `start`, `stop`, `restart`, `status`, `logs`, `attach` |
| **에이전트** | `agent list/add/show/reset/remove/provider` |
| **팀** | `team list/add/add-agent/remove-agent/visualize` |
| **프로바이더** | `provider list/add/remove`, `model` |
| **채팅룸** | `chatroom <team>` (실시간 TUI 뷰어) |
| **페어링** | `pairing pending/approve/unpair` |

---

## 9. 인프라 확장 (tinyclaw-infra)

별도 레포(`shwdsun/tinyclaw-infra`)에서 Docker 오케스트레이션을 제공:

```
┌─────────────────────────────────┐
│  core network (TinyClaw 내부)    │
│  ┌───────┐  ┌──────┐  ┌─────┐ │
│  │ Queue │  │ API  │  │ Web │ │
│  └───────┘  └──────┘  └─────┘ │
├─────────────────────────────────┤
│  gateway (Node.js built-ins only)│ ← npm 의존성 없음, 감사 용이
├─────────────────────────────────┤
│  workers network (격리)          │
│  ┌─────┐ ┌─────┐ ┌─────┐      │
│  │ A-1 │ │ A-2 │ │ A-3 │      │
│  └─────┘ └─────┘ └─────┘      │
│  (gateway만 접근 가능)           │
└─────────────────────────────────┘
```

- Redis 없음 — SQLite로 충분 (단일 노드)
- 워커 컨테이너는 게이트웨이만 접근 가능 (네트워크 격리)

---

## 10. Gastown 생태계와의 비교

| 차원 | TinyClaw | Gas Town | Goosetown |
|------|----------|----------|-----------|
| **목적** | 24/7 개인 AI 비서 팀 | 자율 소프트웨어 개발 공장 | 리서치 병렬화 |
| **채널** | Discord, Telegram, WhatsApp, Web | CLI (tmux) | CLI (Goose) |
| **에이전트 수** | 2~10 | 20~30 | 3~5 |
| **감독** | 없음 (인간이 채팅으로 지시) | Mayor/Witness/Deacon 계층 | Orchestrator (플랫) |
| **머지** | 없음 | Refinery (자동) | 없음 (수동) |
| **장애 복구** | 재시도 5회 + dead-letter | Daemon 자동 재시작 | 없음 |
| **메모리** | SQLite 큐 + 대화 이력 | Beads + Dolt | Beads + gtwall |
| **통신** | 팀 채팅룸 (비동기) | Mail 시스템 (priority) | gtwall (브로드캐스트) |
| **대시보드** | TinyOffice (Next.js, 8개 기능) | htmx + Bubbletea TUI | Starlette + lit-html |
| **시각화** | Office View (책상 시뮬레이션) | 없음 | Village Map (거위) |
| **접근 제어** | Sender Pairing | 없음 (로컬) | 없음 (로컬) |
| **기술 스택** | Node.js/Bash | Go | Python/Bash |
| **복잡도** | 중간 | 높음 (300+ Go 파일) | 낮음 (4,500줄) |

### 핵심 차이

**TinyClaw는 "메시징 앱에서 AI 팀에게 일을 시키는" 사용자 경험**에 초점을 맞춘다. Gas Town은 "인간 없이 에이전트가 알아서 돌아가는" 자율성에 초점을 맞춘다.

TinyClaw의 장점:
- **멀티 채널**: 텔레그램에서 "@coder 버그 고쳐줘"라고 치면 바로 동작
- **접근 제어**: Sender Pairing으로 누가 에이전트에 접근 가능한지 관리
- **낮은 진입 장벽**: 원라인 설치 + 인터랙티브 위자드

TinyClaw의 한계:
- **자율 감독 없음**: 에이전트가 죽거나 정체되면 인간이 알아차려야 함
- **머지 없음**: 에이전트들이 같은 코드베이스를 수정하는 상황 미고려
- **worktree 격리 아님**: 디렉토리 격리는 있지만 Git worktree 기반이 아님

### OpenGoose에의 시사점

TinyClaw의 **멀티 채널 입력**과 **Sender Pairing** 패턴은 OpenGoose에 직접 참고할 수 있다. 특히 Telegram/Discord 통합은 "에이전트에게 지시를 보내는 UX"를 크게 개선하며, Gas Town의 CLI-only 접근 방식의 약점을 보완한다.

---

## 11. 참고 자료

- [GitHub - TinyAGI/tinyclaw](https://github.com/TinyAGI/tinyclaw)
- [TinyOffice 디렉토리](https://github.com/TinyAGI/tinyclaw/tree/main/tinyoffice)
- [TinyOffice README](https://github.com/TinyAGI/tinyclaw/blob/main/tinyoffice/README.md)
- [tinyclaw-infra (Docker)](https://github.com/shwdsun/tinyclaw-infra)
- [@jianxliao TinyOffice 소개](https://x.com/jianxliao/status/2025947616476008629)
- [TinyClaw Setup Guide](https://tinyclawguide.com/)
