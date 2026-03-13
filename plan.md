# OpenGoose v2 Phase 1: Witness + Beads Implementation Plan

> **최종 수정:** 2026-03-13  
> **스토리지:** prollytree (전면 전환), SQLite는 레거시/마이그레이션 전용  
> **원칙:** Beads 컨셉 100% 채택, 순수 Rust 단일 바이너리

Scope: Deliverable 1 (Witness), Deliverable 2 (Beads Data Model), Deliverable 3 (Beads Core Algorithms)

---

## Architectural Decision

### Storage Strategy (전면 전환)

```
Primary:    prollytree (GitHub main v0.3.2-beta)
            - 순수 Rust, 단일 바이너리
            - 구조적 공유, O(변경) diff
            - 3-way merge + ConflictResolver 내장
            - Git-backed 버전 관리

Legacy:     SQLite + Diesel
            - 기존 데이터 마이그레이션 전용
            - Beads 신규 기능에는 사용하지 않음

No Dolt:    외부 서버 의존성 완전 제거
```

### Data Layout (prollytree K-V)

```
work_item:{hash_id}     → JSON { title, status, priority, parent_path, ... }
rel:{from_id}:{to_id}   → JSON { type: "blocks" | "depends_on" | "waits_for", metadata }
memory:{agent}:{key}    → String (agent memory)
wisp:{session}:{id}     → JSON { ... } (ephemeral, burn/squash 대상)
digest:{id}             → JSON { summary, original_count } (squash 결과)
```

### Beads 컨셉 채택

- **Hash-based ID**: SHA-256 + base36, `bd-` prefix, 적응형 길이 (4/6/8자)
- **Materialized Path**: `bd-a3f8.1.1` 형식 계층 표현
- **DAG 기반 의존성**: blocks, depends_on, waits-for, conditional-blocks
- **Wisp**: 로컬 전용 임시 작업 (squash/burn)
- **ready/prime/compact**: 핵심 3대 알고리즘

---

## Crate Layering (의존성 규칙)

```
Layer 0: opengoose-types (공유 도메인 타입 + 트레잇)
    ↓
Layer 1: opengoose-persistence, opengoose-secrets, opengoose-profiles, opengoose-projects
    ↓
Layer 2: opengoose-core, opengoose-provider-bridge
    ↓
Layer 3: opengoose-teams
    ↓
Layer 4: opengoose-discord, opengoose-telegram, opengoose-slack, opengoose-tui,
         opengoose-web, opengoose-cli, opengoose-team-tools
```

**핵심 규칙:**
- 하위 레이어는 상위 레이어에 의존 금지
- prollytree/Diesel은 `opengoose-persistence`에서만 사용
- 프롬프트 포맷팅/오케스트레이션 정책은 `core`/`teams`에 배치
- `opengoose-team-tools`는 독립 MCP 바이너리, `core`/`teams` 의존 금지

**검증:** `cargo tree -i <crate>`로 상향 의존성 확인

---

## Deliverable 1: Witness Module (Dead Agent Detection)

**Goal:** Detect stuck/zombie agents during team execution via EventBus monitoring.

### New Files
- `crates/opengoose-teams/src/witness.rs` — Witness task + WitnessHandle

### Modified Files
- `crates/opengoose-types/src/events.rs` — Add `AgentStuck`, `AgentZombie` to `AppEventKind`
- `crates/opengoose-teams/src/lib.rs` — Add `mod witness; pub use witness::*;`
- `crates/opengoose-teams/Cargo.toml` — Add `dashmap = { workspace = true }`

### Design
- `WitnessConfig`: `stuck_timeout` (default 300s), `zombie_timeout` (default 600s)
- `AgentStatus`: agent_name, team_name, state (Idle/Working/Stuck/Zombie), last_event_at
- `spawn_witness(event_bus, config) -> WitnessHandle`:
  1. `EventBus::subscribe_reliable()` 구독
  2. TeamStepStarted → Working, TeamStepCompleted/Failed → Idle
  3. 모든 AgentEvent → last_event_at 갱신
  4. 5초 타이머로 stuck/zombie 체크
- **GUPP 감지**: Hook에 작업이 있는데 실행하지 않는 에이전트 탐지

### Tests
- Mock EventBus + `tokio::time::advance()` → AgentStuck/AgentZombie 이벤트 검증

---

## Deliverable 2: Beads Data Model (prollytree)

### 2a: Hash ID + ProllyBeadsStore

**New files:**
- `crates/opengoose-persistence/src/prolly/mod.rs` — ProllyBeadsStore
- `crates/opengoose-persistence/src/prolly/hash_id.rs` — Hash ID 생성

**Cargo.toml:**
```toml
[dependencies]
prollytree = { git = "https://github.com/zhangfengcdt/prollytree.git", default-features = false, features = ["git"] }
sha2 = "0.10"
```

**Algorithm:** `SHA-256(title + created_at_nanos + nonce)`, base36, `bd-` prefix  
**Adaptive length:** 4 chars (<500), 6 chars (500-50K), 8 chars (>50K)

**Tests:** prefix format, uniqueness, adaptive length, collision retry

### 2b: Relationships + DAG

**New file:** `crates/opengoose-persistence/src/prolly/relationships.rs`

**K-V Layout:**
```
rel:{from_id}:{to_id} → { type, metadata, created_at }
```

**RelationType enum:**
- Workflow (blocking): Blocks, DependsOn, ConditionalBlocks, WaitsFor
- Association (non-blocking): RelatesTo, Duplicates, Supersedes, DiscoveredFrom

**Methods:** `add_relation`, `remove_relation`, `get_blockers`, `get_dependents`, `has_cycle`

**Dependencies:** `petgraph = "0.8"` (workspace)

**Tests:** blocks, depends_on, direct cycle, transitive cycle, waits-for gate

### 2c: Traits in opengoose-types

```rust
// opengoose-types/src/beads.rs
pub trait BeadsRead {
    fn ready(&self, opts: &ReadyOptions) -> anyhow::Result<Vec<WorkItem>>;
}

pub trait BeadsPrimeSource {
    fn prime_snapshot(&self, team_run_id: &str, agent_name: &str) -> anyhow::Result<PrimeSnapshot>;
}

pub trait BeadsMaintenance {
    fn compact(&self, team_run_id: &str, older_than: DateTime<Utc>) -> anyhow::Result<()>;
}
```

---

## Deliverable 3: Beads Core Algorithms

### 3a: ready()

**Impl:** `crates/opengoose-persistence/src/prolly/ready.rs`

`ready(team_run_id, options) -> Vec<WorkItem>`:
1. NOT ephemeral (wisp 제외)
2. NOT blocked by any open item
3. depends_on 모두 satisfied
4. waits-for 게이트 cleared (all/any children)
5. NOT deferred (defer_until > now)
6. NOT already assigned (configurable)
7. Order by priority ASC, created_at ASC
8. Limit by batch_size (default 10)

**3-step algorithm:** active collection → dependency collection → filtering + caching

**Tests:** 10 cases (empty, blocked, dependency, priority, batch, waits-for, deferred)

### 3b: prime()

**Split:**

**Part 1 - Data (persistence):**
```rust
pub struct PrimeSnapshot {
    pub active: Vec<PrimeSectionItem>,
    pub ready: Vec<PrimeSectionItem>,
    pub recently_completed: Vec<PrimeSectionItem>,
    pub blocked: Vec<(PrimeSectionItem, Vec<String>)>,
    pub memories: Vec<(String, String)>,
}
```

**Part 2 - Formatting (core):**
```rust
pub fn format_prime(snapshot: &PrimeSnapshot, token_budget: usize) -> String {
    // MCP mode: ~50 tokens
    // CLI mode: ~1-2k tokens
}
```

**핵심:** `prime_snapshot()`은 **데이터만** 반환. LLM 프롬프트 포맷, 토큰 예산, MCP/CLI 모드는 `format_prime`에서 처리.

**Tests:** sections, token budget, MCP/CLI mode

### 3c: Wisp (Ephemeral Tasks)

**K-V Layout:**
```
wisp:{session}:{hash_id} → { title, agent, status, ... }
digest:{hash_id}         → { summary, original_count, created_at }
```

**Methods:**
- `create_wisp(session, title, agent)` → ephemeral work item
- `burn_wisp(id)` → hard DELETE (no trace)
- `squash_wisp(id, summary)` → create digest, delete original
- `promote_wisp(id, new_title)` → move to work_item, clear ephemeral
- `purge_ephemeral(session)` → delete closed wisps

**Constraints:** Wisps cannot have relationships

**Tests:** create, excluded from ready, promote, purge, squash creates digest

### 3d: compact()

**Impl:** `crates/opengoose-persistence/src/prolly/compact.rs`

`compact(team_run_id, older_than)`:
1. Group completed items by parent
2. Store digest
3. Mark originals as compacted

**Two-tier:**
- Tier 1: 30+ days → 70% reduction
- Tier 2: 90+ days + Tier 1 → 95% reduction

**Tests:** grouping, digest storage, compacted excluded from ready/prime

---

## Implementation Order (최적화)

```
1. Witness (독립, 스키마 변경 없음)
2. Hash ID + ProllyBeadsStore 기반 구조
3. Relationships + DAG
4. ready() ← 핵심 가치, 빠른 제공
5. prime() ← 컨텍스트 압축, Landing 지원
6. Wisp ← ready/prime 정제
7. compact() ← 장기 성능
```

**변경 이유:** ready()/prime()이 가장 큰 가치 제공. Wisp는 이후 정제 단계.

---

## Migration: SQLite → prollytree

```bash
# One-shot migration CLI
opengoose db migrate-to-prolly --source ~/.opengoose/db.sqlite --target ~/.opengoose/beads

# 검증
opengoose beads verify --compare-sqlite ~/.opengoose/db.sqlite
```

**Migration 후:** SQLite Beads 경로 deprecated, 런타임에서 제거

---

## Future Phases (Reference)

### Phase 1.5: MCP Team Tools
- gtwall 스타일 브로드캐스트: `team.broadcast(message)`
- Agent Map Message Flow 연동

### Phase 2: 격리 및 머지
- per-agent Git worktree
- "re-imagine" 머지 충돌 해결
- prollytree branch/commit/merge 활용

### Phase 3: 규모 확장
- 20+ 에이전트 리소스 관리
- Deacon 패턴 (백그라운드 유지보수)
- OTEL 텔레메트리

### Phase 4: 연합 (Wasteland 패턴)
- Stamps 다차원 평판 (Quality, Reliability, Creativity + severity)
- Trust Ladder (outsider → maintainer)
- Yearbook Rule (`stamped_by != agent_name`)
- 멀티 인스턴스 연합 (prollytree 3-way sync)

**상세:** `docs/20-architecture/v2-master.md` §7 참조

---

## Reference: Source Patterns

| 패턴 | 출처 | 적용 |
|------|------|------|
| gtwall 브로드캐스트 | Goosetown | MCP team.broadcast() |
| Village Map | Goosetown | Agent Map 실시간 시각화 |
| GUPP (Pull 기반 실행) | Gastown | Witness hook 미실행 감지 |
| Polecat 상태머신 | Gastown | AgentStatus (Idle/Working/Stuck/Zombie) |
| Mail 시스템 | Gastown | AgentMessageStore 확장 |
| Convoy (작업 번들) | Gastown | orchestration_runs 확장 |
| Hash ID + Adaptive Length | Beads | hash_id.rs |
| Materialized Path | Beads | parent_path 필드 |
| ready/prime/compact | Beads | Deliverable 3 |
| Wisp (ephemeral) | Beads | Deliverable 3c |
| waits-for 게이트 | Beads | FanOut 완료 대기 |
| Yearbook Rule | Wasteland | Phase 4 reviewer 제약 |
| Stamps 평판 | Wasteland | Phase 4 agent_stamps |
| Trust Ladder | Wasteland | Phase 4 신뢰 수준 |
