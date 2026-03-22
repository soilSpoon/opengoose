# CoW Store: In-Memory Branch/Merge for Board WorkItems

**Date:** 2026-03-22
**Status:** Design approved
**Crate:** `opengoose-board`

## Problem

Multiple Rigs writing to the Board simultaneously can cause data conflicts. The current SQLite-backed Board has no isolation — every write goes directly to the shared state. A branching mechanism is needed so each Rig works in isolation and merges results back safely.

## Design Summary

In-memory `Arc<BTreeMap<i64, WorkItem>>` with Copy-on-Write semantics (Rust `Arc::make_mut`), inspired by Dolt's prolly tree. SQLite remains as the persistence layer. Only WorkItems go through CoW; Stamps and Relations stay in SQLite (structurally conflict-free).

## Architecture

```
┌─ In-Memory (Fast) ───────────────────────────┐
│                                              │
│  CowStore                                    │
│  ├── main: Arc<BTreeMap<i64, WorkItem>>      │
│  ├── branches: HashMap<RigId, Branch>        │
│  └── commit_log: Vec<Commit>                 │
│                                              │
│  Branch = snapshot of main at creation time  │
│  Arc::make_mut() = CoW on first write        │
│                                              │
└──────────────┬───────────────────────────────┘
               │ On every merge: write main to SQLite (WAL mode)
┌──────────────▼───────────────────────────────┐
│  SQLite (Durability)                         │
│  ├── work_items table (main state snapshot)  │
│  ├── commit_log table (hash chain)           │
│  ├── stamps table (unchanged)                │
│  └── relations table (unchanged)             │
└──────────────────────────────────────────────┘
```

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Storage | `Arc<BTreeMap>` in-memory + SQLite persistence | Prolly tree overkill for current scale (dozens of items). `Arc::make_mut()` gives CoW for free. |
| Branch API | Explicit `Branch` handle | Rust-idiomatic; type system enforces branch lifecycle; no hidden state. |
| Read isolation | Snapshot (Dolt/Git-style) | All references (Beads, Dolt, GasTown) use snapshot isolation. Branch sees data at creation time only. |
| Branch granularity | Per-Rig (not per-WorkItem) | Matches Beads/GasTown. Branch lives for Rig's session, not one task. |
| Merge model | CRDT `Mergeable` trait | Fields are conflict-free by construction. No ad-hoc resolution rules. Compiler enforces merge impl for new fields. |
| Commit log | SHA-256 hash chain | Audit trail + rollback capability. Stored in SQLite on every merge. |
| Persistence timing | On every merge (SQLite WAL) | Merge is infrequent (end of Rig session). WAL handles write performance. "Commit = persisted" guarantee. |
| Scope | WorkItem only | Stamps (yearbook rule) and Relations (append-only facts) are structurally conflict-free. No branching needed. |
| ID assignment | SQLite auto-increment via `post()` on main | Branch operations only modify existing items. New items are created on main directly, then visible to branches on next branch creation. Avoids in-memory vs SQLite ID conflicts. |

## Data Structures

### CowStore

```rust
pub struct CowStore {
    main: Arc<BTreeMap<i64, WorkItem>>,
    branches: HashMap<RigId, Branch>,
    commits: Vec<Commit>,
}
```

### Branch

```rust
pub struct Branch {
    name: RigId,
    data: Arc<BTreeMap<i64, WorkItem>>,  // snapshot at creation, CoW on write
    base_commit: CommitId,               // where this branch diverged from main
}
```

### Commit

```rust
/// Unique identifier for a commit in the hash chain.
pub struct CommitId(pub u64);

pub struct Commit {
    pub id: CommitId,
    pub parent: Option<CommitId>,
    pub root_hash: [u8; 32],     // SHA-256 of main state
    pub branch: RigId,           // which branch merged
    pub message: String,
    pub timestamp: DateTime<Utc>,
}
```

## Branch Lifecycle

```
1. Worker starts session
   → let branch = store.branch(&rig_id);
   // Arc::clone of main — O(1), zero copy

2. Worker operates through Branch handle
   → branch.claim(item_id);        // Arc::make_mut triggers CoW on first write
   → branch.submit(item_id, result);

3. Worker session ends (or periodic commit)
   → let result = store.merge(branch);
   // 3-way: base(branch creation snapshot) vs branch vs current main
   // Returns MergeResult with convergence log
   // Writes main to SQLite (WAL)
   // Appends to commit log

4. Worker fails
   → store.discard(branch);
   // main unaffected, branch data dropped
```

## Explicit Branch API

```rust
impl CowStore {
    /// Create a snapshot branch for a Rig. O(1) via Arc::clone.
    pub fn branch(&mut self, rig_id: &RigId) -> Branch;

    /// 3-way merge: base vs branch vs main. Writes to SQLite on success.
    pub fn merge(&mut self, branch: Branch) -> Result<MergeResult>;

    /// Discard a branch without merging (on failure/abandon).
    pub fn discard(&mut self, branch: Branch);

    /// Restore main state from SQLite on startup.
    pub async fn restore(db: &DatabaseConnection) -> Result<Self>;

    /// Get current commit log.
    pub fn commits(&self) -> &[Commit];
}

impl Branch {
    /// Read a work item. Returns branch copy if modified, otherwise base snapshot.
    pub fn get(&self, id: i64) -> Option<&WorkItem>;

    /// List all work items visible to this branch.
    pub fn list(&self) -> impl Iterator<Item = &WorkItem>;

    /// Filter ready items (open + unblocked by given set of blocked IDs).
    pub fn ready(&self, blocked_ids: &HashSet<i64>) -> Vec<&WorkItem>;

    /// Write operations — triggers CoW on first call.
    pub fn insert(&mut self, item: WorkItem);
    pub fn update(&mut self, id: i64, f: impl FnOnce(&mut WorkItem));
    pub fn remove(&mut self, id: i64);
}
```

## CRDT Merge Model

Each mutable field on WorkItem implements `Mergeable`. Merge is conflict-free by construction — no "conflict resolution", only mathematical convergence.

### Trait

```rust
/// Conflict-free merge of two diverged values.
///
/// Implementations must satisfy:
/// - Commutativity: a.merge(b) == b.merge(a)
/// - Associativity: a.merge(b.merge(c)) == a.merge(b).merge(c)
/// - Idempotency:   a.merge(a) == a
pub trait Mergeable {
    fn merge(&self, other: &Self) -> Self;
}
```

### Field Strategies

| Field Type | CRDT | Behavior | Fields |
|------------|------|----------|--------|
| `Status` | LWW-Register | Latest `updated_at` wins (see Status note below) | status |
| `Priority` | Max-register | Higher urgency wins: P0 > P1 > P2. Escalation only — de-escalation must happen on main directly. | priority |
| `Tags` | G-Set (grow-only) | Union + dedup | tags |
| `Option<String>` scalars | LWW-Register | Latest `updated_at` wins | claimed_by |
| Immutable fields | N/A | Never diverge — same value on both sides | id, title, description, project, parent, created_by, created_at |

**Status note:** Status is NOT a join-semilattice because the state machine allows backward transitions (Claimed→Open via unclaim, Stuck→Open via retry). Using `max()` would silently discard intentional backward transitions like retry. Therefore Status uses LWW-Register (`updated_at` comparison), which preserves the most recent intentional state change regardless of direction.

### Implementation

```rust
impl Mergeable for Status {
    fn merge(&self, other: &Self) -> Self {
        // LWW — delegated to LwwField wrapper at the WorkItem merge level.
        // Status itself has no standalone merge; it is always wrapped in LwwField<Status>.
        unreachable!("Status merges through LwwField<Status>")
    }
}

impl Mergeable for Priority {
    fn merge(&self, other: &Self) -> Self {
        std::cmp::max(*self, *other)
    }
}

impl Mergeable for Tags {
    fn merge(&self, other: &Self) -> Self {
        let mut union: BTreeSet<String> = self.0.iter().cloned().collect();
        union.extend(other.0.iter().cloned());
        Tags(union.into_iter().collect())
    }
}

/// Last-Write-Wins register. Ties go to `self` (deterministic).
impl<T: Clone> Mergeable for LwwField<T> {
    fn merge(&self, other: &Self) -> Self {
        if self.updated_at >= other.updated_at { self.clone() } else { other.clone() }
    }
}
```

### WorkItem Merge (3-way)

```rust
/// 3-way merge: base (common ancestor) vs branch vs main
pub fn merge_work_item(base: &WorkItem, branch: &WorkItem, main: &WorkItem) -> MergedItem {
    let mut convergences = Vec::new();

    // For each mutable field:
    // - If only one side changed from base → take that side
    // - If both sides changed → use Mergeable::merge()
    // - Record convergence for audit

    let status = merge_lww_field("status", &base.status, &branch.status, &main.status, &mut convergences);
    let priority = merge_field("priority", &base.priority, &branch.priority, &main.priority, &mut convergences);
    let tags = merge_field("tags", &base.tags, &branch.tags, &main.tags, &mut convergences);
    let claimed_by = merge_lww_field("claimed_by", &base.claimed_by, &branch.claimed_by, &main.claimed_by, &mut convergences);
    // ...

    MergedItem {
        item: WorkItem { status, priority, tags, claimed_by, /* ... immutable fields from base */ },
        convergences,
    }
}
```

### Merge Result

```rust
pub struct MergeResult {
    pub merged_items: Vec<MergedItem>,
    pub commit: Commit,
}

pub struct MergedItem {
    pub item_id: i64,
    pub convergences: Vec<Convergence>,
}

pub struct Convergence {
    pub field: &'static str,
    pub branch_value: String,
    pub main_value: String,
    pub converged_to: String,
    pub strategy: MergeStrategy,
}

pub enum MergeStrategy {
    OneSided,      // only one side changed
    MaxRegister,   // Priority
    GrowSet,       // Tags
    LastWriteWins, // LWW scalar fields (Status, claimed_by, etc.)
}
```

## Commit Log (Hash Chain)

```rust
impl CowStore {
    fn compute_root_hash(data: &BTreeMap<i64, WorkItem>) -> [u8; 32] {
        // SHA-256 over sorted (id, serialized WorkItem) pairs
        let mut hasher = Sha256::new();
        for (id, item) in data.iter() {
            hasher.update(id.to_le_bytes());
            hasher.update(serde_json::to_vec(item).unwrap());
        }
        hasher.finalize().into()
    }

    fn append_commit(&mut self, branch: &RigId, message: String) -> Commit {
        let root_hash = Self::compute_root_hash(&self.main);
        let parent = self.commits.last().map(|c| c.id);
        let commit = Commit {
            id: CommitId(self.commits.len() as u64),
            parent,
            root_hash,
            branch: branch.clone(),
            message,
            timestamp: Utc::now(),
        };
        self.commits.push(commit.clone());
        commit
    }
}
```

## SQLite Persistence

### On Merge (Write)

```rust
impl CowStore {
    async fn persist(&self, db: &DatabaseConnection) -> Result<()> {
        // Transaction: upsert changed work_items + append commit log
        db.transaction(|txn| {
            // 1. For each item in self.main:
            //    INSERT OR REPLACE INTO work_items
            // 2. INSERT commit into commit_log
            //
            // Note: uses upsert (INSERT OR REPLACE) rather than DELETE-all + INSERT-all
            // to preserve rowid stability and avoid unnecessary writes.
            // Future optimization: track dirty items during merge and only persist changed items.
        }).await
    }
}
```

### On Startup (Restore)

```rust
impl CowStore {
    pub async fn restore(db: &DatabaseConnection) -> Result<Self> {
        let items: BTreeMap<i64, WorkItem> = load_all_work_items(db).await?;
        let commits: Vec<Commit> = load_commit_log(db).await?;
        Ok(CowStore {
            main: Arc::new(items),
            branches: HashMap::new(),
            commits,
        })
    }
}
```

### New Tables

```sql
CREATE TABLE commit_log (
    id          INTEGER PRIMARY KEY,
    parent_id   INTEGER REFERENCES commit_log(id),
    root_hash   BLOB NOT NULL,          -- 32 bytes SHA-256
    branch      TEXT NOT NULL,           -- rig_id
    message     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
```

`work_items` table schema unchanged — it stores the latest main state snapshot.

### New Entity File

New file: `entity/commit_log.rs` — SeaORM entity for commit_log table.
Add to `entity/mod.rs` exports.

## Board Integration

Board struct adds `store: CowStore` field. Stamps, relations, rigs, and notifications continue to use `self.db` directly, as they do today.

```rust
pub struct Board {
    store: CowStore,                  // WorkItem — branch/merge target (in-memory)
    db: DatabaseConnection,           // SQLite for stamps, relations, rigs, persistence
    notify: Arc<Notify>,              // Work item notifications
    stamp_notify: Arc<Notify>,        // Stamp notifications
}

impl Board {
    pub fn branch(&mut self, rig_id: &RigId) -> Branch {
        self.store.branch(rig_id)
    }

    pub async fn merge(&mut self, branch: Branch) -> Result<MergeResult> {
        let result = self.store.merge(branch)?;
        self.store.persist(&self.db).await?;
        Ok(result)
    }

    pub fn discard_branch(&mut self, branch: Branch) {
        self.store.discard(branch);
    }

    /// Post a new work item. Goes directly to main (SQLite assigns ID).
    /// Branches see new items on next branch creation.
    pub async fn post(&mut self, item: NewWorkItem) -> Result<i64> {
        let id = insert_to_sqlite(&self.db, &item).await?;
        // Also insert into in-memory main
        Arc::make_mut(&mut self.store.main).insert(id, item.into_work_item(id));
        Ok(id)
    }
}
```

### Worker Integration

```rust
// Worker.run() — updated flow
pub async fn run(&self) {
    loop {
        // Compute blocked IDs from SQLite relations (before branching)
        let blocked_ids = self.board.blocked_item_ids().await?;

        // Create branch — snapshot of main at this moment
        let mut branch = self.board.branch(&self.id);

        // Find and process work
        if let Some(item_id) = branch.ready(&blocked_ids).first().map(|i| i.id) {
            branch.update(item_id, |item| item.claim(&self.id));
            // ... process work ...
            branch.update(item_id, |item| item.submit(result));
        }

        // Merge branch back to main + persist to SQLite
        let merge_result = self.board.merge(branch).await?;
        // Log convergences if any

        // Wait for new work notification
        self.notify.notified().await;
    }
}
```

## Files to Create/Modify

### New files in `crates/opengoose-board/src/`:

| File | Purpose | ~Lines |
|------|---------|--------|
| `store.rs` | `CowStore` struct, `Arc<BTreeMap>` CoW, persist/restore | ~200 |
| `branch.rs` | `Branch` handle, read/write ops, snapshot isolation | ~150 |
| `merge.rs` | `Mergeable` trait, `LwwField<T>`, 3-way merge, `MergeResult` | ~250 |
| `entity/commit_log.rs` | SeaORM entity for `commit_log` table | ~30 |

### Modified files:

| File | Changes |
|------|---------|
| `board.rs` | Add `store: CowStore` field. WorkItem reads/writes go through CowStore. Add `branch()`, `merge()`, `discard_branch()`. `post()` writes to both SQLite and in-memory main. |
| `work_item.rs` | Add `Mergeable` impls for `Priority`, `Tags`. Wrap `status` and `claimed_by` in `LwwField<T>` for merge support. |
| `entity/mod.rs` | Add `commit_log` module export. |
| `beads.rs` | `filter_ready()` and `prime_summary()` accept `&BTreeMap<i64, WorkItem>` (from Branch or main CowStore). |

### Unchanged files:

| File | Reason |
|------|--------|
| `stamps.rs`, `stamp_ops.rs` | Stays in SQLite. Yearbook rule = no conflicts. |
| `relations.rs` | Stays in SQLite. Append-only facts = no conflicts. |
| `rigs.rs` | Rig registration unrelated to CoW. |

## Scaling Path

Current: `Arc<BTreeMap>` handles dozens to thousands of WorkItems efficiently.

Future: When multiple concurrent Rigs cause real contention or WorkItem count exceeds practical in-memory limits, replace `CowStore` internals with Dolt backend behind the same `Board` API. No consumer code changes needed.

## Out of Scope

- `compact()` — Phase 5 (AI summarization of old items)
- `trust_level()` calculation changes — independent of CoW
- Memory layer (`board__remember`, `board__recall`) — Phase 2
- Federation / DoltHub integration — Phase 2+
