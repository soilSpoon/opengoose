# steveyegge/beads — 소스 코드 분석

> 분석일: 2026-03-11
> 레포: https://github.com/steveyegge/beads
> 언어: Go | 라이선스: MIT | 버전: v0.59.0 | Stars: 18.7k+

---

## 1. 프로젝트 개요

Beads는 **AI 코딩 에이전트를 위한 분산, Git 기반 그래프 이슈 트래커**다. 마크다운 계획서를 **의존성 인식 작업 그래프(dependency-aware task graph)**로 대체하여, 에이전트가 확장된 워크플로우 동안 컨텍스트를 유지할 수 있게 하는 **영속적, 구조화된 메모리** 레이어를 제공한다.

> "Beads는 AI 코딩 에이전트를 위한 메모리 업그레이드다." — steveyegge/beads README

## 2. 핵심 아키텍처

### 데이터베이스
- **Dolt 기반**: 버전 관리 SQL, 셀 레벨 머지, 네이티브 브랜칭, 리모트 동기화
- **스키마**: v6 (issues, dependencies, labels, comments, events, interactions, metadata)

### ID 체계
- **해시 기반** (`bd-a1b2`): 멀티 에이전트/멀티 브랜치에서 머지 충돌 원천 차단
- **계층 지원**: 중첩 작업 구조 (`bd-a3f8` → `bd-a3f8.1` → `bd-a3f8.1.1`)

### 관계 유형
- `relates_to` — 관련 작업 연결
- `duplicates` — 중복 작업 마킹
- `supersedes` — 대체 작업 표시
- `replies_to` — 메시지 스레딩

## 3. `bd` CLI 핵심 명령어

### 작업 관리

| 명령어 | 용도 |
|--------|------|
| `bd init` | 프로젝트에 Beads 초기화 |
| `bd init --stealth` | 로컬 전용 (repo에 커밋 안 함) |
| `bd init --contributor` | 포크된 repo에서 별도 계획 repo로 라우팅 |
| `bd create "Title" -p 0 -t task` | 우선순위 0 작업 생성 |
| `bd update <id> --claim` | 원자적으로 배정 + in-progress 마킹 |
| `bd dep add <child> <parent>` | 의존성 링크 |
| `bd close <id>` | 작업 완료 |

### 에이전트 최적화 명령어

| 명령어 | 용도 |
|--------|------|
| `bd ready` | **열린 차단자 없는 작업만** 표시 (에이전트 토큰 절약) |
| `bd prime` | AI 최적화된 프로젝트 컨텍스트 |
| `bd show <id>` | 전체 상세 + 감사 추적 |
| `bd blocked` | 차단된 작업 표시 |
| `bd list --status in_progress` | 진행 중인 작업 |

## 4. 핵심 기능

### Agent-Optimized
- JSON 출력 지원
- 의존성 추적으로 작업 순서 자동 결정
- `bd ready`로 실행 가능한 작업만 필터링 → 에이전트 토큰 절약

### Compaction (압축) — 시맨틱 메모리 감쇠
- 오래된 닫힌 작업을 자동으로 요약
- 컨텍스트 윈도우 절약
- 작업 이력의 "메모리 감쇠" 시뮬레이션

### Messaging
- 메시지 이슈 유형 (`issue_type='message'`)
- 스레딩 (`--thread`)
- 임시(ephemeral) 생명주기

### Zero Conflict
- 해시 기반 ID로 멀티 에이전트 워크플로우에서 머지 충돌 원천 차단
- Dolt의 셀 레벨 머지와 결합하여 안전한 동시 작업

## 5. 에이전트 세션 패턴

### 세션 시작
```
bd prime          ← 프로젝트 컨텍스트 로드
bd ready          ← 실행 가능한 작업 확인
bd list --status in_progress  ← 이전에 시작된 작업 확인
```

### 세션 종료
```
bd blocked        ← 차단된 작업 기록
[잔여 작업 상태 업데이트]
```

## 6. Gas Town vs Goosetown에서의 사용

| 측면 | Gas Town | Goosetown |
|------|----------|-----------|
| **역할** | 핵심 메모리 레이어 — 모든 작업 상태, 에이전트 ID, CV, 이벤트 | 크래시 복구 + 진행 추적 |
| **통합 깊이** | `internal/beads/beads.go` 래퍼, 재시도 로직, 크로스 rig 참조 | `bd` CLI 직접 호출 |
| **Orchestrator 사용** | Mayor가 Convoy로 beads 관리, Sling으로 배정 | Orchestrator가 이슈 생성, delegate가 업데이트 |
| **Researcher 사용** | N/A (워커 전용) | `goosetown-researcher-beads` skill로 `--readonly` 검색 |
| **DB 레벨** | Two-level: Town-level (`hq-*`) + Rig-level (`gt-*`, `bd-*`) | 단일 레벨 |

### Gas Town에서의 Two-level Beads

```
~/.gt/.beads/           ← Town-level (hq-* 프리픽스)
                           크로스 rig 조율 (Mayor, Deacon, convoys)

<rig>/mayor/rig/.beads/ ← Rig-level (gt-*, bd-* 프리픽스)
                           프로젝트 작업
```

### Gas Town의 Beads 래퍼 (internal/beads/beads.go)
- `bd` CLI를 Go에서 호출하는 추상화 레이어
- 재시도 로직: 10회, 최대 30초, ±25% 지터
- 크로스 rig 참조: `external:prefix:id` 형식
- transient lock 에러 vs config 에러 구분

## 7. 데이터 생명주기

```
CREATE → LIVE → CLOSE → DECAY → COMPACT → FLATTEN
```

- **CREATE**: 작업 생성 (해시 ID 자동 부여)
- **LIVE**: 활성 작업 (in-progress, 코멘트, 업데이트)
- **CLOSE**: 완료된 작업
- **DECAY**: 오래된 닫힌 작업 (시맨틱 감쇠 시작)
- **COMPACT**: 요약으로 압축 (원본 보존, 컨텍스트 절약)
- **FLATTEN**: 최종 아카이브

## 8. 설치

```bash
npm install -g @beads/bd          # npm
brew install beads                # Homebrew
go install github.com/steveyegge/beads/cmd/bd@latest  # Go
```

## 9. 커뮤니티

- **Stars**: 18.7k+ (2026.03 기준)
- **Rust 포트**: `Dicklesworthstone/beads_rust` (`br` CLI) — 커뮤니티 주도
- **주요 기여자**: Matt Wilkie (공동 메인테이너 예정)
- Anthropic의 2025년 11월 논문 "장기 실행 에이전트를 위한 효과적인 하네스"에서도 동일한 패턴 제안
