# OpenGoose v2 Phase 1: Witness + Beads Implementation Plan

Scope: Deliverable 1 (Witness), Deliverable 2 (Beads Data Model), Deliverable 3 (Beads Core Algorithms)

## Architectural Decision

All Beads code goes into `opengoose-persistence` (not a new crate). The existing `WorkItem` model is the Beads analog. New tables share the same migration chain and `Database::with()` pattern.

---

## Deliverable 1: Witness Module (Dead Agent Detection)

**Goal:** Detect stuck/zombie agents during team execution via EventBus monitoring.

### New Files
- `crates/opengoose-teams/src/witness.rs` — Witness task + WitnessHandle

### Modified Files
- `crates/opengoose-types/src/events.rs` — Add `AgentStuck { team, agent }` and `AgentZombie { team, agent }` variants to `AppEventKind`, plus Display/key/source_gateway impls
- `crates/opengoose-teams/src/lib.rs` — Add `mod witness; pub use witness::*;`
- `crates/opengoose-teams/Cargo.toml` — Add `dashmap = { workspace = true }`

### Design
- `WitnessConfig` struct with configurable `stuck_timeout` (default 300s) and `zombie_timeout` (default 600s)
- `AgentStatus` tracks: agent_name, team_name, state (Idle/Working/Stuck/Zombie), last_event_at, started_at
- `spawn_witness(event_bus, config) -> WitnessHandle` spawns a tokio task that:
  1. Subscribes via `EventBus::subscribe_reliable()` (unbounded mpsc)
  2. On TeamStepStarted → registers agent as Working
  3. On TeamStepCompleted/Failed → marks Idle
  4. On ModelChanged/ContextCompacted/ExtensionNotification → updates last_event_at
  5. Every 5s tick, checks Working agents for stuck/zombie thresholds
- `WitnessHandle` holds `Arc<DashMap<String, AgentStatus>>` for read access

### Tests
- Unit tests with mock EventBus: emit TeamStepStarted, advance time via `tokio::time::advance()`, verify AgentStuck/AgentZombie emitted

---

## Deliverable 2: Beads Phase 1 — Data Model

### 2a: Hash ID

**New file:** `crates/opengoose-persistence/src/hash_id.rs`

Algorithm: `SHA-256(title + created_at_nanos + nonce)`, base36-encoded with `bd-` prefix. Adaptive length: 4 chars (<500 items), 6 chars (500-50K), 8 chars (>50K). Collision retry with incrementing nonce.

**Modified files:**
- `Cargo.toml` (workspace) — add `sha2 = "0.10"`
- `crates/opengoose-persistence/Cargo.toml` — add `sha2`
- `crates/opengoose-persistence/src/schema.rs` — add `hash_id` to `work_items` table
- `crates/opengoose-persistence/src/models.rs` — add `hash_id` to `WorkItemRow`, `NewWorkItem`
- `crates/opengoose-persistence/src/work_items.rs` — add `hash_id: Option<String>` to `WorkItem`, update `create()`
- `crates/opengoose-persistence/src/lib.rs` — add `mod hash_id;`

**New migration:** `migrations/YYYYMMDD_add_beads_columns/up.sql`
```sql
ALTER TABLE work_items ADD COLUMN hash_id TEXT UNIQUE;
ALTER TABLE work_items ADD COLUMN is_ephemeral INTEGER NOT NULL DEFAULT 0;
ALTER TABLE work_items ADD COLUMN priority INTEGER NOT NULL DEFAULT 3;
```

**Tests:** prefix format, uniqueness, adaptive length (2 thresholds), collision retry, deterministic

### 2b: Relationships + DAG

**New file:** `crates/opengoose-persistence/src/relationships.rs`

**New migration:** `migrations/YYYYMMDD_add_work_item_relations/up.sql`
```sql
CREATE TABLE work_item_relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_item_id INTEGER NOT NULL REFERENCES work_items(id),
    to_item_id INTEGER NOT NULL REFERENCES work_items(id),
    relation_type TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(from_item_id, to_item_id, relation_type)
);
```

`RelationStore` methods: `add_relation`, `remove_relation`, `get_blockers`, `get_dependents`, `has_cycle` (DFS-based cycle detection).

`RelationType` enum via `db_enum!`: Blocks, DependsOn, RelatesTo, Duplicates.

**Dependencies:** Add `petgraph = "0.8"` to workspace + persistence crate.

**Modified files:**
- `Cargo.toml` (workspace) — add `petgraph`
- `crates/opengoose-persistence/Cargo.toml` — add `petgraph`
- `crates/opengoose-persistence/src/schema.rs` — add `work_item_relations` table
- `crates/opengoose-persistence/src/models.rs` — add `RelationRow`, `NewRelation`
- `crates/opengoose-persistence/src/lib.rs` — add `mod relationships; pub use`

**Tests:** add blocks, add depends_on, detect direct cycle, detect transitive cycle, allow non-cyclic, remove relationship, get_blockers

### 2c: Wisp (Ephemeral Tasks)

Extends `WorkItemStore` with wisp methods:
- `create_wisp(session_key, team_run_id, title, agent)` — `is_ephemeral = 1`
- `burn_wisp(id)` — hard DELETE
- `squash_wisp(id, summary)` — insert digest, DELETE original
- `promote_wisp(id, new_title)` — set `is_ephemeral = 0`
- `purge_ephemeral(team_run_id)` — DELETE closed wisps for a run

**Schema addition** (same migration as 2a):
```sql
CREATE TABLE wisp_digests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    original_wisp_id INTEGER NOT NULL,
    agent_name TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Constraint: wisps cannot have relationships (add_relation rejects ephemeral items).

**Tests:** create sets ephemeral, excluded from ready, promote to task, purge clears closed, purge keeps open

---

## Deliverable 3: Beads Phase 2 — Core Algorithms

### 3a: ready()

**New file:** `crates/opengoose-persistence/src/ready.rs`

`ready(team_run_id, options) -> Vec<WorkItem>` returns pending items that:
1. Are NOT ephemeral
2. Are NOT blocked by any open item
3. Have all depends_on satisfied (targets completed)
4. Are NOT already assigned (configurable)
5. Ordered by priority ASC, created_at ASC
6. Limited by batch_size (default 10)

Implementation uses SQL NOT EXISTS subqueries.

**Tests:** 10 tests covering empty, single ready, blocked excluded, dependency satisfied, priority ordering, batch limit, etc.

### 3b: prime()

**New file:** `crates/opengoose-persistence/src/prime.rs`

`prime(team_run_id, agent_name) -> String` generates minimal context for agent system prompts:
```
# Active Tasks (assigned to you)
- [bd-a3f8] Refactor auth middleware (in_progress)

# Ready Tasks (available)
- [bd-f7a2] Add rate limiting (pending, priority: 2)

# Recently Completed (last 5)
- [bd-c1d4] Fix login handler (completed, 2h ago)

# Blocked
- [bd-e5f6] Deploy to staging (blocked by: bd-a3f8)
```

Target: <2000 tokens for 100 tasks.

**Tests:** 9 tests covering format, sections, token budget, etc.

### 3c: compact()

**New file:** `crates/opengoose-persistence/src/compact.rs`

`compact(team_run_id, older_than)` summarizes old completed tasks:
1. Groups completed items by parent
2. Stores digest in `work_item_compacted` table
3. Marks originals with `status = 'compacted'` (new WorkStatus variant)

**Schema** (in same migration as 2a):
```sql
CREATE TABLE work_item_compacted (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id INTEGER,
    summary TEXT NOT NULL,
    item_count INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Add `Compacted => "compacted"` to WorkStatus db_enum.

**Tests:** compact groups items, compacted excluded from ready/prime, digest stored correctly

---

## Implementation Order

1. Deliverable 1: Witness (no schema changes, independent)
2. Deliverable 2a: Hash ID + schema migration (foundation)
3. Deliverable 2b: Relationships + DAG
4. Deliverable 2c: Wisp
5. Deliverable 3a: ready()
6. Deliverable 3b: prime()
7. Deliverable 3c: compact()

Each step builds on previous ones and is independently testable. `cargo check` will be run after each deliverable to ensure compilation.
