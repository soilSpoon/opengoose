# Wasteland: Distributed Agent Federation

Wasteland는 분산된 에이전트 인스턴스 간의 작업 조정, 평판 관리, 연합 프로토콜을 정의한다.

## Core Concepts

### 1. Wanted Board (작업 게시판)
작업 게시판의 상태 머신:

```
open → claimed → in_review → completed
  ↓                           ↑
  └─ withdrawn    accept (+ stamp)
       ↓             or
     deleted      close (no stamp)
```

- **open**: 사용 가능한 작업
- **claimed**: 에이전트가 작업 시작
- **in_review**: 증거 URL과 함께 제출, 수락 대기
- **completed**: 수락되어 평판 스탬프 획득
- **withdrawn**: 게시자가 철회

### 2. Stamps (다차원 평판)
평판은 단일 점수가 아닌 다차원 벡터:

**Valence (품질 차원):**
- Quality (1-5): 코드 품질
- Reliability (1-5): 신뢰성
- Creativity (derived): 혁신성

**Severity (가중치):**
- leaf = 1 point (작은 작업)
- branch = 3 points (중요한 작업)
- root = 5 points (핵심/기반 작업)

**Weighted Score:**
```sql
SUM(CASE severity 
    WHEN 'root' THEN 5 
    WHEN 'branch' THEN 3 
    WHEN 'leaf' THEN 1 
END) AS weighted_score
```

### 3. Trust Ladder (신뢰 수준)
가중 점수에 따른 자동 승급:

| Level | Name | Score | Capabilities |
|-------|------|-------|--------------|
| L1 | outsider | < 3 | 신규, 검증 안 됨 |
| L1.5 | newcomer | ≥ 3 | 첫 기여 완료 |
| L2 | contributor | ≥ 10 | 검증된 기록 |
| L2.5 | trusted | ≥ 25 | 고가치 기여 |
| L3 | maintainer | ≥ 50 | 기반 작업, 다른 사람 검토 가능 |

### 4. Yearbook Rule (자기 검토 방지)
자신의 작업은 자신이 검증할 수 없다:

**DB Constraint:**
```sql
CHECK (NOT(author = subject))  -- self-stamping impossible
```

**Logic Layer:**
```go
case TransitionAccept:
    return item.PostedBy == actor && item.ClaimedBy != actor
```

결과: 작업 완료자와 검토자는 반드시 다른 rig이어야 함.

### 5. Federation Protocol
분산 인스턴스 간 연합:

**Fork-based Model:**
```
upstream commons (hop/wl-commons)
    ↑ (pull request)
    │
origin fork (alice-dev/wl-commons) ← [local clone]
```

**HOP URI (Portable Identity):**
```
hop://alice@example.com/alice-rig/
```

- 인스턴스 간 이동해도 ID 유지
- Trust Level은 인스턴스별 저장
- Stamps는 HOP URI를 포함하여 크로스 인스턴스 평판 조회 가능

**Workflow Modes:**
- **PR Mode**: 변경은 브랜치로, PR로 upstream에 제출
- **Wild-West**: maintainer는 직접 push

### 6. Rig Links (크로스 인스턴스 연결)
다른 인스턴스의 동일 소유자 rig 연결:

```sql
CREATE TABLE rig_links (
    rig_a VARCHAR(255) NOT NULL,
    rig_b VARCHAR(255) NOT NULL,
    link_type VARCHAR(32),     -- 'same_owner', 'parent-child'
    status VARCHAR(32),        -- 'pending', 'verified', 'revoked'
);
```

## OpenGoose Phase 4 적용

| Wasteland | OpenGoose |
|-----------|-----------|
| Wanted Board | orchestration_runs + work_items |
| Stamps | agent_stamps 테이블 |
| Trust Ladder | weighted_score → trust_level 계산 |
| Yearbook Rule | `CHECK (stamped_by != agent_name)` |
| Federation | RemoteAgent WebSocket + prollytree sync |
| HOP URI | agent_id + instance_id 조합 |

### agent_stamps Schema
```sql
CREATE TABLE agent_stamps (
    id INTEGER PRIMARY KEY,
    agent_name TEXT NOT NULL,
    work_item_id INTEGER NOT NULL,
    dimension TEXT NOT NULL,      -- quality, reliability, creativity
    score REAL NOT NULL,          -- -1.0 to 1.0
    severity TEXT NOT NULL,       -- leaf, branch, root
    stamped_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (work_item_id) REFERENCES work_items(id),
    CHECK (stamped_by != agent_name)  -- Yearbook Rule
);
```

---

*Source: [github.com/gastownhall/wasteland](https://github.com/gastownhall/wasteland)*
