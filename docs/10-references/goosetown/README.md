# Goosetown Architecture Summary

Goosetown은 Block의 **Goose 프레임워크** 기반 멀티에이전트 오케스트레이션 시스템이다. Gastown(Go 75k LOC)과 달리 Goose의 Skill 시스템과 delegate() API를 활용하여 더 가벼운 구조를 가진다.

## Core Philosophy

### 1. Conductor-and-Instruments Model
오케스트레이터는 작업을 분해하고 전문화된 Delegate들을 병렬로 실행한다:

```
        Orchestrator (main session)
             │ spawns
      ┌──────┼──────┬──────┐
      ▼      ▼      ▼      ▼
   Researchers  Workers  Writers  Reviewers
      │         │        │        │
      └─────────┴────────┴────────┘
              via gtwall (broadcast)
```

### 2. Delegation >> Doing
오케스트레이터는 직접 코드를 작성하지 않고 위임만 수행한다. 빌드/연구를 시작하는 순간 리더십을 멈춘 것이다.

### 3. Flat Hierarchy
Delegate는 sub-delegate를 생성할 수 없다. 무한 재귀 방지 및 책임 명확화.

## Key Mechanisms

### gtwall (Town Wall)
Bash 파일 기반 append-only 브로드캐스트 채널 (~400줄):

- **Storage**: `$GOOSE_GTWALL_FILE` per session
- **Lock**: 디렉토리 생성 기반 mutex (30초 stale 감지)
- **Position-tracked reads**: 각 reader가 `.pos` 파일로 위치 추적

**Message Format:**
```
timestamp|sender_id|message
[HH:MM:SS] <sender_id> message
```

**Cadence (flock 필수 규칙):**
1. 시작 시: 작업 내용 공유 (🎬 starting)
2. 3-5 tool calls마다: 새 메시지 읽기
3. 발견 즉시: 결과 공유 (💡 discovery)
4. 완료 시: 요약 공유 (✅ done)

### Skill System (Goose Skills)
역할별 독립된 instruction set (Goose의 `.goose/skills/` 디렉토리에 정의):

| Role | Skill | Function |
|------|-------|----------|
| Orchestrator | goosetown-orchestrator | 분해, 위임, 합성 |
| Researcher | 8 variants | 정보 수집 (read-only) |
| Worker | goosetown-worker | 빌드 실행 |
| Writer | goosetown-writer | 문서화 |
| Reviewer | goosetown-reviewer | 평가 (read-only) |

**필수 Preamble:**
```
You are <name>. Your gtwall ID is <name>.
FIRST ACTION: Run ./gtwall --usage and follow those instructions throughout your work.
```

### Village Map
실시간 에이전트 시각화:

- 역할별 건물 배치 (Hall, Library, Factory, Scriptorium 등)
- A* 경로 계산으로 에이전트 이동 애니메이션
- SSE 기반 실시간 업데이트
- gtwall 메시지를 speech bubble로 표시

### Telepathy (Paging)
긴급 메시지 전달:
- `$GOOSE_MOIM_MESSAGE_FILE`에 직접 쓰기
- Wrap-up 경고: ⏰ (5분) → 🚨 (60초, STOP)

## OpenGoose 적용

| Goosetown | OpenGoose v2 |
|-----------|--------------|
| gtwall | MCP team.broadcast() |
| Village Map | Agent Map (SSE + Askama) |
| Skill System | Team 정의 + 역할별 Recipe |
| Telepathy | EventBus urgent message |
| delegate() | Runner.spawn_agent() |

---

*Source: [github.com/block/goosetown](https://github.com/block/goosetown)*
