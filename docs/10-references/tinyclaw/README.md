# TinyClaw Architecture Summary

TinyClaw는 Steve Yegge의 **경량 멀티에이전트 오케스트레이션** 시스템으로, Gas Town(75k LOC Go)의 교훈을 더 작은 규모로 재구현한 프로젝트다. Gas Town과 Goosetown과는 별개의 독립 프로젝트.

## Core Components

### TinyOffice (Office View)

TinyClaw의 웹 대시보드. **실용적 정보 밀도**가 핵심 가치 — 한 화면에서 모든 에이전트의 상태를 즉시 파악할 수 있다.

**제공하는 정보:**
- 에이전트별 카드: 이름, 현재 작업 제목, 상태 (active/idle/stuck), 경과 시간
- 작업 큐 상태: pending → processing → completed / dead
- 에이전트 간 메시지 흐름 타임라인
- 실시간 업데이트

**Goosetown Village Map과의 차이:**

| | TinyOffice | Goosetown Village Map |
|---|---|---|
| **핵심 가치** | 정보 밀도 (한 화면에 모든 상태) | 시각적 생동감 (에이전트가 살아있는 느낌) |
| **시각화** | 테이블/카드 기반, 컴팩트 | A* 경로 애니메이션, 건물 배치, 말풍선 |
| **데이터** | 작업 제목, 상태, 경과 시간 | 역할별 위치, 메시지 말풍선 |
| **복잡도** | 낮음 (단순 폴링) | 높음 (~700줄 village.js) |
| **적합한 상황** | 운영 모니터링 | 데모/프레젠테이션 |

### SQLite WAL Queue

TinyClaw의 작업 큐는 SQLite WAL 모드 기반:

```
pending → processing → completed
                    ↘ dead (실패 시)
```

- 메시지 상태 추적: `MessageStatus { Pending, Processing, Completed, Failed, Dead }`
- 재시도 로직: `retry_count`, `max_retries`
- Dead-letter queue: 실패한 메시지 추적

## OpenGoose 적용

| TinyClaw | OpenGoose v2 |
|----------|--------------|
| TinyOffice (에이전트 카드) | Agent Map — 에이전트 카드 (정보 밀도) |
| TinyOffice (작업 큐) | MessageQueue (이미 구현, 동일 패턴) |
| SQLite WAL 큐 | MessageQueue + AgentMessageStore (이미 구현) |

**Agent Map 설계에서의 역할:**
- TinyOffice → **정보 밀도** 차용: 에이전트 이름, 팀, 상태(Working/Idle/Stuck/Zombie), 경과 시간을 카드 한 장에 압축
- Goosetown Village Map → **시각적 생동감** 차용: SSE 실시간 업데이트, 상태 변화 애니메이션, 메시지 말풍선
- 두 프로젝트의 장점을 결합하여 **운영에 실용적이면서도 시각적으로 생동감 있는** 대시보드 구현

---

*Note: TinyClaw는 Gas Town, Goosetown과 별개의 독립 프로젝트.*
