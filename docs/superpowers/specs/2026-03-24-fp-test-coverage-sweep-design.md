# FP + Test Coverage Sweep v3 — Design Spec

**Date:** 2026-03-24
**Branch:** `soilSpoon/fp-test-coverage`
**Approach:** Hybrid (Approach C) — leaf crate stabilization → large file decomposition → cross-cutting cleanup

## Constraints

- No users, no deployments — full breaking changes allowed
- Public API signatures, module paths, trait definitions all mutable
- Completion criteria: `cargo test --workspace` green, `cargo clippy --workspace` clean

## Phase 1: Leaf Crate Stabilization

### 1a. opengoose-board

**Decomposition:**
- `work_items/transitions.rs` (470 lines) — extract pure transition functions, separate validation from state mutation
- `work_items/helpers.rs` (459 lines) — group by function: filtering, sorting, formatting
- `merge.rs` (407 lines) — already solid; strengthen proptest coverage per Mergeable impl

**Tests to add:**
- `store/` module — CowStore branching/snapshot unit tests (currently undertested)
- `transitions.rs` — illegal transition boundary tests
- `merge.rs` — additional proptest cases for commutativity/associativity

**FP improvements:**
- Remove remaining `.unwrap()` calls
- Replace mutable accumulators with fold/collect where possible

### 1b. opengoose-skills

**Decomposition:**
- `manage/promote.rs` (504 lines) — split path resolution / metadata / file ops into submodules
- `manage/add.rs` (488 lines) — separate copy logic from validation
- `manage/discover/mod.rs` (370 lines) — split by discovery strategy (filesystem scan vs registry)
- `loader.rs` (396 lines) — separate pure catalog building from filesystem I/O

**Tests to add (biggest gap — only ~30 tests currently):**
- `manage/` — promote, add, remove, discover edge cases
- `loader.rs` — malformed frontmatter, empty files, duplicate skill names
- `evolution.rs` — proptest for lifecycle transitions (active→dormant→archived)

**FP improvements:**
- Abstract filesystem dependency via trait → in-memory impl for tests (no mocks)
- Unify skill filtering/sorting into iterator chains
- Standardize `anyhow` error handling patterns

## Phase 2: Large File Decomposition

### 2a. evolver/pipeline.rs (1,177 lines → 3-4 modules)

| New module | Responsibility | Nature |
|-----------|---------------|--------|
| `evolver/context.rs` | Stamp context preparation, skill pair building | Pure |
| `evolver/process.rs` | Stamp processing, retry logic | I/O + orchestration |
| `evolver/effectiveness.rs` | Effectiveness update logic | Pure |
| `evolver/pipeline.rs` | Top-level orchestration only | ~200 lines |

### 2b. evolver/sweep.rs (997 lines → 3 modules)

| New module | Responsibility | Nature |
|-----------|---------------|--------|
| `evolver/sweep/decision.rs` | Decision parsing, apply logic | Pure |
| `evolver/sweep/dormant.rs` | Dormant skill search, effectiveness summary | Pure |
| `evolver/sweep/mod.rs` | Sweep loop orchestration | ~200 lines |

### 2c. tui/event/keys.rs (844 lines → 3 modules)

| New module | Responsibility | Nature |
|-----------|---------------|--------|
| `tui/event/keymap.rs` | Key binding definitions | Data/pure |
| `tui/event/handler.rs` | Event dispatch logic | Logic |
| `tui/event/keys.rs` | Public interface only | Thin |

### 2d. main.rs (785 lines → modular CLI)

| New module | Responsibility | Nature |
|-----------|---------------|--------|
| `cli/commands.rs` | Subcommand handlers | I/O |
| `cli/setup.rs` | Initialization, config loading | I/O |
| `main.rs` | Entrypoint + clap definitions | ~150 lines |

**Decomposition principles:**
- Extracted modules are pure functions where possible
- I/O stays in orchestrators only
- Unit tests accompany every extraction

## Phase 3: opengoose-rig + Cross-Cutting

### 3a. opengoose-rig decomposition

- `rig/worker.rs` (290 lines) — separate pull loop orchestration from pure claim/submit logic
- `worktree.rs` (463 lines) — separate RAII guard from git commands; extract pure path resolution
- `middleware.rs` (414 lines) — each middleware as independent module with shared trait
- `conversation_log/io.rs` (305 lines) — separate pure retention policy from I/O

### 3b. opengoose-rig tests

- `worker.rs` — claim → process → submit/abandon flow unit tests
- `worktree.rs` — path resolution, branch name generation pure function tests
- `middleware.rs` — independent tests per middleware
- `conversation_log/` — retention policy proptest (file size × age combinations)

### 3c. Cross-cutting (all crates)

- Remove all 36 remaining `.unwrap()` → `.expect()` or `?`
- Review `.expect()` message quality — include debugging context
- Standardize per-crate `Error` enum
- Remove unnecessary `mut` bindings
- Minimize `.clone()` — borrow where possible

## Completion Criteria

| Metric | Target |
|--------|--------|
| `cargo test --workspace` | Green |
| `cargo clippy --workspace` | Warning-free |
| Max file size | ≤400 lines |
| New tests added | 50+ |
| Remaining `.unwrap()` | 0 |
| All modules | Pure logic separated from I/O |
