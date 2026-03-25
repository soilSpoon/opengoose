# FP + Test Coverage Sweep v3 — Design Spec (Revised)

**Date:** 2026-03-24
**Branch:** `soilSpoon/fp-test-coverage`
**Approach:** Impact-driven — side-effect separation → error handling → test gaps → structural improvements

## Constraints

- No users, no deployments — full breaking changes allowed
- Public API signatures, module paths, trait definitions all mutable
- Completion criteria: `cargo test --workspace` green, `cargo clippy --workspace` clean

## Guiding Principle

**Refactor by concern separation, not by file size.** A 1,000-line file with good cohesion is better than 4 files with tangled imports. Decompose when the seam is a real domain boundary; refactor internally when coupling is tight.

---

## Phase 1: Side-Effect Separation (Core FP Fix)

The highest-impact FP improvement: untangle pure computation from I/O in the hottest code paths.

### 1a. evolver/pipeline.rs — pure context vs I/O orchestration

**Problem:** `prepare_context()` mixes pure skill-pair building with `board.post()` + `board.claim()`. If claim fails, orphan work item. `execute_action()` mixes LLM call + file write + board state change — 3 layers of side-effects with no atomicity.

**Fix (internal refactor, keep as one file):**
- Extract `fn build_context(stamp, skills, log) -> PreparedContext` (pure — returns struct, no I/O)
- Extract `fn validate_and_parse_response(raw) -> Result<ParsedAction>` (pure)
- Keep `async fn run_pipeline()` as the only function that calls board/filesystem/LLM
- Pattern: pure functions compute what to do, orchestrator does it

**Tests:** Unit test `build_context()` and `validate_and_parse_response()` with edge cases (empty skills, malformed responses, missing stamps)

### 1b. skills/evolution/writer — pure metadata vs file writes

**Problem:** `write_skill_to_rig_scope()` and `update_existing_skill()` mix name parsing (pure), metadata building (pure), and file I/O in one function. Can't test parsing/metadata logic without writing files.

**Fix:**
- Extract `fn build_skill_metadata(name, version, source) -> SkillMetadata` (pure)
- Extract `fn parse_skill_name(content) -> Result<String>` (pure)
- Extract `fn compute_version_bump(existing_meta) -> u32` (pure)
- Keep `write_*` functions thin: call pure functions, then write results

**Tests:** Unit test all pure extractors. Proptest `compute_version_bump` with arbitrary metadata.

### 1c. skills/loader.rs — pure catalog building vs filesystem scan

**Problem:** `scan_scope()` takes `&mut Vec`, `&mut HashSet` — mutation-based side-effects. Filesystem I/O deeply nested.

**Fix:**
- Extract `fn build_catalog(scoped_skills: Vec<(Scope, Vec<RawSkill>)>) -> Vec<LoadedSkill>` (pure — scope override, dedup, sorting)
- `scan_scope()` returns `Vec<RawSkill>` instead of mutating arguments
- Caller composes: scan each scope → build_catalog

**Tests:** Unit test `build_catalog()` with scope override scenarios (rig > project > global), duplicate names, empty scopes.

### 1d. CowStore API — fix false mutation pattern

**Problem:** `branch()` takes `&mut self` but is semantically read-only (Arc clone). `discard()` is a no-op that just drops the branch.

**Fix:**
- `fn branch(&self) -> Branch` — take shared ref, return new Branch via Arc::clone
- Remove `discard()` entirely — callers can just `drop(branch)` (idiomatic Rust)
- Keep `insert`, `update`, `remove` as `&mut self` (these really mutate)

**Tests:** Verify `branch()` from shared ref compiles; snapshot isolation proptest.

---

## Phase 2: Error Handling Fix

### 2a. Silent error swallowing (42 instances)

**Problem:** `.ok()` and `let _ =` discard errors silently. `worker.rs` alone has 7 `board.abandon().await.ok()` — DB failures are invisible.

**Fix by category:**

| Pattern | Count | Action |
|---------|-------|--------|
| `board.abandon().await.ok()` | 7 | `warn!()` + context on error, continue |
| `let _ = tx.send()` | 6 | `warn!()` if channel closed (tui/event/rigs.rs + tui/tui_layer.rs) |
| `let _ = apply_decision()` | 1 | `sweep.rs:137` — log + continue |
| `board.*.await.ok()` in main.rs | 3 | `main.rs:734-766` — background worker spawns |
| `.ok()` on file reads | 2 | Log + return default |
| Other `.ok()` / `let _ =` | ~23 | Case-by-case: log or propagate |

**Principle:** No silent drops. If you intentionally ignore an error, log it with `warn!()` and context.

### 2b. Cross-crate error consistency

**Current state:**
- `opengoose-board`: `BoardError` enum (good)
- `opengoose-skills`: `anyhow::Result` everywhere (opaque)
- `opengoose-rig`: mixed `anyhow` + ad-hoc

**Fix:**
- Each crate gets a `Error` enum with meaningful variants
- `opengoose-skills`: `SkillError { LoadFailed, InvalidFrontmatter, EvolutionFailed, ... }`
- `opengoose-rig`: `RigError { SessionFailed, WorktreeFailed, BoardError(BoardError), ... }`
- `From` impls for crate boundary conversions

### 2c. `.unwrap()` baseline

Non-test code already has 0 `.unwrap()` calls (all 36 are in `#[cfg(test)]` modules — acceptable).
No action needed. Maintain this baseline.

---

## Phase 3: Test Gap Coverage

### 3a. Test infrastructure (do first — reduces boilerplate for everything after)

Create `#[cfg(test)]` fixture modules per crate:

```
opengoose-board/src/test_fixtures.rs
  - make_work_item(id, status, priority) → WorkItem
  - make_stamp(rig_id, dimension, score) → Stamp
  - in_memory_board() → Board (pre-connected)

opengoose-skills/src/test_fixtures.rs
  - make_skill(name, scope) → LoadedSkill
  - make_metadata(generated_at, version) → SkillMetadata
  - temp_skill_dir() → TempDir with valid structure

opengoose-rig/src/test_fixtures.rs
  - temp_home() → TempDir + HOME override
  - mock_agent() → impl Agent
```

### 3b. High-priority untested pub functions

| Module | Untested functions | Test strategy |
|--------|-------------------|---------------|
| `board/stamp_ops.rs` | 10 (weighted_score, trust_level, batch_rig_scores, ...) | Unit tests with fixtures; proptest weighted_score monotonicity |
| `board/rigs.rs` | 4 (register, list, get, remove) | Unit tests with in_memory_board |
| `board/queries.rs` | 5 (get, list, ready, claimed_by, completed_by_rig) | Unit tests; boundary tests (empty board, no matches) |
| `rig/work_mode.rs` | 3 (chat, task, with_session_id) | Unit tests for session ID generation |
| `rig/worktree.rs` | 2 (create, attach) | Pure path tests; skip git I/O in unit tests |
| `skills/manage/*` | promote, remove, list | Filesystem fixture tests |

### 3c. Weak test upgrades

Migrate 34 `.is_ok()`-only assertions to value checks:
- Board stamp ops: verify actual scores, not just "didn't error"
- Web API tests: verify response bodies, not just status 200
- Skills loader: verify loaded skill contents match expected

### 3d. Property testing expansion

| Invariant | Module |
|-----------|--------|
| `∀a,b: serialize(merge(a,b)) == serialize(merge(b,a))` | merge.rs |
| `∀item: deserialize(serialize(item)) == item` | work_item.rs |
| `∀meta: json_roundtrip(meta) == meta` | metadata.rs |
| `∀scopes: rig scope always overrides project/global` | loader.rs |
| `∀status: terminal states reject all transitions` | transitions.rs |
| `∀retention_policy: files_to_keep ⊆ all_files` | conversation_log |

### 3e. Error path tests (currently 0 across workspace)

- Board: invalid transitions, cycle detection on complex graphs, concurrent claims
- Skills: malformed frontmatter, corrupt metadata.json, permission denied
- Rig: initialization failures, missing workdirs, cancelled tokens
- Pipeline: LLM returns garbage, board down during pipeline

---

## Phase 4: Structural Improvements

### 4a. keys.rs — command dispatch table redesign

**Problem:** `handle_key` dispatch is ~145 lines of match logic (file is 844 lines total, 745 are tests). Not urgent, but the match-based design doesn't scale and handlers aren't individually testable.

**Fix:**
```rust
type KeyHandler = fn(&mut App, &Arc<Board>) -> KeyResult;

fn build_keymap() -> HashMap<(KeyCode, KeyModifiers), KeyHandler> {
    let mut map = HashMap::new();
    map.insert((KeyCode::Char('q'), NONE), handle_quit);
    map.insert((KeyCode::Tab, NONE), handle_tab);
    // ...
    map
}
```
- Each handler is a small named function (3-10 lines)
- Keymap is data, handlers are logic — clean separation
- Easy to test individual handlers

### 4b. Type safety — validated newtypes

| Type | Validation | Where |
|------|-----------|-------|
| `SkillName(String)` | non-empty, no `/`, no `..`, ≤64 chars | opengoose-skills |
| `RigId(String)` | existing newtype, add validation to `new()` | opengoose-board |
| `SessionId(String)` | non-empty, safe chars | opengoose-rig |
| `Dimension` enum | `Quality`, `Relevance`, `Accuracy`, ... | opengoose-board |

`TryFrom<String>` for each — validation at construction, not at use site.

### 4c. Clone minimization (top offenders)

| Location | Fix |
|----------|-----|
| `worktree.rs:34-38` — 3 PathBuf clones | Destructure `self` into owned fields |
| `mcp_tools/handlers.rs` — 5 rig_id clones | Use `Arc<RigId>` or `Cow` |
| `web/api/skills.rs` — 7 metadata clones | Build response from `&SkillMetadata` refs |
| `rig/mod.rs` — session_config clones | Arc already wraps it; deref instead |

### 4d. Unnecessary `mut` removal + `fold`/`collect` conversion

Scan all `let mut` bindings; convert accumulation loops to iterator chains where readability improves.

---

## Phase 5: Targeted File Decomposition (only where domain boundaries exist)

Only decompose files where the split follows a real domain seam:

| File | Action | Rationale |
|------|--------|-----------|
| `main.rs` (785) | Extract `cli/commands.rs`, `cli/setup.rs` | CLI handlers are independent domain from entrypoint |
| `web/api/skills.rs` (867) | Extract `SkillContext` methods to pure functions; keep in one file | High cohesion — all skill HTTP endpoints. Reduce size via internal refactor |
| `web/api/board.rs` (525) | Keep as-is | Acceptable size, good cohesion |
| `pipeline.rs` (1,177) | Internal refactor only (Phase 1a) | Don't split — tight internal coupling |
| `sweep.rs` (997) | Keep as-is | Perfect cohesion, no split needed |

**Relaxed target:** ≤600 lines per file (not 400). Files with good cohesion stay whole.

---

## Completion Criteria

| Metric | Target |
|--------|--------|
| `cargo test --workspace` | Green |
| `cargo clippy --workspace` | Warning-free |
| Max file size | ≤600 lines (cohesive files exempt) |
| New tests added | 80+ |
| `.unwrap()` in non-test code | 0 (already met — maintain) |
| Silent error drops (`.ok()`, `let _ =`) | 0 (all logged) |
| Untested pub function ratio | <20% (from current 32%) |
| Side-effect separation | Pure functions extracted in pipeline, writer, loader |
| Type-safe newtypes | SkillName, RigId (validated), SessionId, Dimension |
