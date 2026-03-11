# block/goosetown — 소스 코드 분석

> 분석일: 2026-03-11
> 레포: https://github.com/block/goosetown
> 언어: Python/Bash | 규모: ~4,500줄 코어 코드

---

## 1. 프로젝트 개요

Goosetown은 Block의 Goose CLI 위에 구축된 **미니멀 멀티 에이전트 오케스트레이션 레이어**다. Gas Town에서 영감받았지만 의도적으로 단순하게 설계되어 **리서치 퍼스트 병렬 작업**에 최적화되어 있다.

> "Goosetown은 Goose의 subagent 업그레이드를 한계까지 밀어붙이기 위한 재미있는 실험으로 시작됐다. 일상 도구로 사용하기 시작하자 얼마나 잘 작동하는지에 완전히 놀랐다." — Tyler Longwell

## 2. 디렉토리 구조

```
block/goosetown/
├── goose                       ← 진입점 (43줄 Bash) — 환경 설정 후 Goose CLI 실행
├── goose_gui                   ← macOS GUI 래퍼
├── gtwall                      ← 브로드캐스트 벽 (~400줄 순수 Bash)
├── dashboard                   ← 대시보드 런처 (screen 기반 백그라운드)
├── scripts/
│   ├── goosetown-ui            ← 대시보드 서버 (~610줄 Python, Starlette + SSE)
│   ├── build-catalog           ← YAML 프론트매터에서 CATALOG.md 생성 (150+ 줄)
│   └── validate-map            ← 맵 유효성 검사
├── ui/
│   ├── js/
│   │   ├── state.js            ← 옵저버 패턴 상태 관리
│   │   ├── village.js          ← A* 경로탐색으로 거위 애니메이션 (~700줄)
│   │   ├── buildings.js        ← 역할→건물 매핑
│   │   └── [components]        ← lit-html 컴포넌트 (Registry, Bulletin, Workshop, Clockworks)
│   ├── css/                    ← 스팀펑크 테마 (~30KB)
│   └── index.html, village.html, editor.html
├── .claude/skills/             ← 12개 역할 정의 (YAML + Markdown)
├── tests/                      ← Bash/Python 테스트 (~470줄)
├── AGENTS.md                   ← 에이전트 규칙
└── pyproject.toml              ← starlette, uvicorn, pyyaml
```

## 3. 4가지 핵심 컴포넌트

### 3.1 Skills — 역할 블루프린트
`.claude/skills/goosetown-*/SKILL.md` (12개)

| Skill | 역할 | 할 수 있는 것 | 할 수 없는 것 |
|-------|------|-------------|-------------|
| **Orchestrator** | 코디네이터 (40KB 지시서) | 분해, delegate 스폰, 합성, beads 관리 | 코드/문서 직접 작성 |
| **Worker** | 빌더 | 코드 작성, 파일 생성, 즉시 실행 | 다른 worker 스폰, 질문 |
| **Writer** | 합성 전문가 | 다수 소스 → 하나의 문서 합성 | 코드 수정 |
| **Reviewer** | 품질 게이트 | 작업 평가, 심각도 점수, 결함 보고 | 직접 수정 |
| **Researchers (8종)** | 도메인별 검색 | 특정 소스 검색, 발견사항 보고 | 파일 수정 (읽기 전용) |

Researcher 종류: local, github, reddit, stackoverflow, arxiv, jira, slack, beads

### 3.2 Subagents — 임시 에이전트 인스턴스
- `summon` 확장의 `delegate()` 함수로 생성
- 자체 클린 컨텍스트에서 작업 → 요약 반환
- `async: true` 옵션으로 비차단 스폰
- 기본 subagent 모델을 저렴한 것으로 설정 가능, ad-hoc 모델 선택도 지원

### 3.3 Beads — 크래시 복구
- `bd` CLI를 직접 호출 (Gas Town처럼 래퍼 없음)
- Orchestrator가 이슈 생성, delegate가 업데이트
- 세션 실패 시 다음 에이전트가 bead를 집어 이어서 작업

### 3.4 gtwall — 유일한 에이전트간 통신 채널

## 4. gtwall 구현 상세 (~400줄 Bash)

```bash
# 핵심 아키텍처:
# Wall 파일: ~/.goosetown/walls/wall-<pid>-<random>.log (append-only)
# 포맷: HH:MM:SS|sender_id|message (파이프 이스케이프: \|)
# 포지션: .positions/<id>.pos (증분 읽기용)

# 동시성 제어: mkdir 기반 디렉토리 락
acquire_lock() {
    mkdir $lockfile  # 원자적 디렉토리 생성 (POSIX 보장)
    # 실패 시: 내부 PID 확인
    # PID 죽었거나 30초 초과 시: stale lock 제거 후 재시도
    # PID와 타임스탬프 기록
}
```

| 명령 | 동작 |
|------|------|
| `gtwall <id> "msg"` | 쓰기 + 읽기 (새 메시지만) |
| `gtwall <id>` | 읽기 전용 |
| `gtwall --reset <id>` | 위치 초기화 (전체 히스토리) |
| `gtwall --clear` | 벽 + 모든 위치 초기화 |
| `gtwall --usage` | 실행 케이던스 출력 |

### 실제 사용 예시

```
[10:14] researcher-api-auth - 🚨 잠재적 쇼스토퍼: 서비스 호출자의 capabilities가 비어있음
[10:14] researcher-endpoints - 💡 발견: 최소 의존성의 네이티브 엔드포인트가 이미 존재
[10:15] researcher-source - ✅ 완료. 네이티브 경로는 새 의존성 제로. 피봇 권장.
```

→ 병렬 리서처 3명이 **1분 안에** 쇼스토퍼 발견 → 대안 → 피봇 합의

## 5. Dashboard 구현

### 백엔드 (`scripts/goosetown-ui`, ~610줄 Python)
- **Starlette** + SSE (Server-Sent Events) 실시간 스트리밍
- 고정 크기 링 버퍼: `deque(maxlen=10000)`
- Goose 세션 DB를 **읽기 전용**으로 열어 delegate 트리 구성
  - `PRAGMA query_only=ON` + `sqlite3.connect(...?mode=ro)`
- 상태 추론: elapsed < 15s = active, < 30s = waiting, > 120s = complete

### 프론트엔드 (lit-html, 빌드 스텝 없음)
- **옵저버 패턴** 상태 관리 (`ui/js/state.js`)
- 컴포넌트: Registry, Bulletin, Workshop, Clockworks, Village, Editor

### Village Map (`ui/js/village.js`, ~700줄)
- **A* 경로탐색**: 거위가 Town Hall에서 역할별 건물로 걸어감
- 타일 비용: cobblestone(1), grass(5), water(∞), buildings(1)
- 애니메이션: 160 px/sec, 말풍선 8초
- 건물 매핑:
  - Orchestrator → Town Hall (H)
  - Researcher → Grand Archive (L)
  - Worker → Cog Factory (C)
  - Reviewer → Inspector's Tower (I)
  - Writer → The Scriptorium (W)

## 6. 워크플로우 5단계

```
1. Research   ← 3~6 병렬 리서처, gtwall에 발견사항 게시
     ↓
2. Process    ← 결과 통합, 갭 식별, 필요시 후속 리서치
     ↓
3. Plan       ← 리서치 합성 → 계획, 워커/라이터 배정 (분리된 파일)
     ↓
4. Review     ← crossfire: 다중 모델 적대적 QA
     ↓
5. Synthesize ← 모든 조각 통합, 리뷰어 발견사항 반영, 충돌 해결
```

### 종료 시퀀스 (Telepathy)
```
Orchestrator: ./gtwall orchestrator "⏰ 5 MIN WARNING"
              echo "📡 ALL: READ GTWALL NOW" > $GOOSE_MOIM_MESSAGE_FILE

Worker가 telepathy 핑 + 벽 메시지 확인 → 발견사항 게시 → 종료

Orchestrator: ./gtwall orchestrator "🚨 STOP. POST FINDINGS NOW"
```

## 7. 진입점 흐름

```
./goose 실행
  ↓
GOOSE_GTWALL_FILE=~/.goosetown/walls/wall-<pid>-<random>.log 생성
GOOSE_MOIM_MESSAGE_FILE=/tmp/goose-telepathy-<pid>.txt 생성
  ↓
AGENTS.md 로드
  ↓
Orchestrator skill 로드: load(source: "goosetown-orchestrator")
  ↓
작업 분해, delegate 스폰: delegate(source: "goosetown-worker", async: true)
  ↓
각 delegate에 필수 지시 주입:
  "You are <name>. Your gtwall ID is <name>.
   FIRST ACTION: Run ./gtwall --usage..."
```

## 8. 핵심 설계 원칙

1. **플랫 계층** — Delegate가 다른 Delegate를 생성할 수 없음
2. **오케스트레이터는 산출물을 만들지 않음** — 컨텍스트를 조율에만 사용
3. **방송 조율, 공유 상태 없음** — 에이전트는 메모리나 도구 출력을 공유하지 않음
4. **부분 결과 > 완전한 침묵** — 취소된 에이전트의 8/10 > 메모리에만 있던 완전한 작업
5. **모델 유연성** — 기본 subagent 모델을 저렴한 것으로 설정 가능

> "단일 모델이라도 다른 컨텍스트의 여러 에이전트 형태로 아이디어를 자동으로 주고받게 하면 결과물이 더 좋아진다." — Tyler Longwell

## 9. 테스트 & CI

```
tests/
├── run_all.sh          ← 드라이버 스크립트
├── test_gtwall.sh      ← Wall append/read/lock 동작
├── test_dashboard.sh   ← 서버 시작, API 엔드포인트
└── test_catalog.py     ← YAML 파싱, 유효성 검사
```

Pre-commit hooks: Black, MyPy, Bandit(보안), Biome(JS), 로컬 테스트
