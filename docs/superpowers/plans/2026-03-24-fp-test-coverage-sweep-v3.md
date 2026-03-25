# FP + Test Coverage Sweep v3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve code quality across 4 crates by separating side-effects from pure logic, fixing silent error drops, closing test coverage gaps, and adding type safety.

**Architecture:** Impact-driven phases — side-effect separation first (creates testable seams), then error handling (gives tests meaningful failures to exercise), then test infrastructure + coverage, then structural improvements. Internal refactoring preferred over file splitting.

**Tech Stack:** Rust, proptest, tokio, sea-orm, anyhow → crate-specific error enums

**Spec:** `docs/superpowers/specs/2026-03-24-fp-test-coverage-sweep-design.md`

---

## File Map

### Phase 1 — Side-Effect Separation
| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `crates/opengoose/src/evolver/pipeline.rs` | Extract pure context builder + response parser |
| Modify | `crates/opengoose-skills/src/evolution/writer/mod.rs` | Extract pure metadata builder + name parser |
| Modify | `crates/opengoose-skills/src/loader.rs` | Return-based scan_scope, pure catalog builder |
| Modify | `crates/opengoose-board/src/store/mod.rs` | Fix CowStore branch(&self), remove discard() |

### Phase 2 — Error Handling
| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `crates/opengoose-rig/src/rig/worker.rs` | Replace 7 .ok() with warn!() logging |
| Modify | `crates/opengoose/src/tui/event/rigs.rs` | Replace 4 let _ = tx.send() |
| Modify | `crates/opengoose/src/tui/tui_layer.rs` | Replace let _ = try_send() |
| Modify | `crates/opengoose/src/web/mod.rs` | Replace let _ = tx2.send() |
| Modify | `crates/opengoose/src/evolver/sweep.rs:137` | Replace let _ = apply_decision() |
| Modify | `crates/opengoose/src/main.rs:41,734-766` | Replace .ok() on board ops |
| Modify | Multiple files | Remaining ~23 .ok()/let _ = instances |
| Create | `crates/opengoose-skills/src/error.rs` | SkillError enum |
| Create | `crates/opengoose-rig/src/error.rs` | RigError enum |

### Phase 3 — Test Infrastructure + Coverage
| Action | File | Responsibility |
|--------|------|---------------|
| Create | `crates/opengoose-board/src/test_fixtures.rs` | Board test builders |
| Create | `crates/opengoose-skills/src/test_fixtures.rs` | Skills test builders |
| Create | `crates/opengoose-rig/src/test_fixtures.rs` | Rig test builders |
| Modify | `crates/opengoose-board/src/stamp_ops.rs` | Add 10+ unit tests |
| Modify | `crates/opengoose-board/src/rigs.rs` | Add 4 unit tests |
| Modify | `crates/opengoose-board/src/work_items/queries.rs` | Add 5 unit tests |
| Modify | `crates/opengoose-rig/src/work_mode.rs` | Add 3 unit tests |
| Modify | Multiple files | Proptest expansion, error path tests |

### Phase 4 — Structural Improvements
| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `crates/opengoose/src/tui/event/keys.rs` | Command dispatch table |
| Modify | `crates/opengoose-board/src/work_item.rs:20-22` | RigId validation |
| Create | `crates/opengoose-skills/src/skill_name.rs` | SkillName newtype |
| Modify | `crates/opengoose-rig/src/work_mode.rs` | SessionId newtype |
| Modify | Multiple files | Clone minimization, mut removal |

### Phase 5 — Targeted Decomposition
| Action | File | Responsibility |
|--------|------|---------------|
| Create | `crates/opengoose/src/cli/commands.rs` | CLI subcommand handlers |
| Create | `crates/opengoose/src/cli/setup.rs` | Runtime init, config |
| Modify | `crates/opengoose/src/main.rs` | Thin entrypoint only |

---

## Task 1: Extract pure context builder from pipeline.rs

**Files:**
- Modify: `crates/opengoose/src/evolver/pipeline.rs:50-146`

- [ ] **Step 1: Write tests for the pure context builder**

In `pipeline.rs`, add to the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn build_context_with_empty_skills_returns_no_existing_section() {
    let stamp = make_test_stamp(1, "quality", 2.0, "test comment");
    let skills: Vec<LoadedSkill> = vec![];
    let log_summary = "user asked about testing";
    let ctx = build_evolve_context(&stamp, &skills, log_summary);
    assert!(!ctx.prompt.contains("Existing skills:"));
    assert!(ctx.prompt.contains("test comment"));
}

#[test]
fn build_context_includes_skill_pairs_in_prompt() {
    let stamp = make_test_stamp(1, "quality", 2.0, "improve");
    let skills = vec![make_test_skill("auto-commit", "Commits automatically")];
    let log_summary = "";
    let ctx = build_evolve_context(&stamp, &skills, log_summary);
    assert!(ctx.prompt.contains("auto-commit"));
    assert!(ctx.prompt.contains("Commits automatically"));
}

#[test]
fn build_context_with_missing_log_uses_empty() {
    let stamp = make_test_stamp(1, "quality", 2.0, "comment");
    let ctx = build_evolve_context(&stamp, &[], "");
    assert!(ctx.prompt.contains("comment"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p opengoose --lib evolver::pipeline::tests::build_context -- 2>&1 | head -20`
Expected: FAIL — `build_evolve_context` not found

- [ ] **Step 3: Extract the pure function**

Extract from the current `prepare_context()` (lines 69-125). Create a `PreparedContext` struct and `build_evolve_context` pure function that takes stamp + skills + log summary and returns the struct with the built prompt. No board calls, no filesystem reads.

The current `prepare_context()` should call `build_evolve_context()` and then perform the board.post() + board.claim() I/O separately.

Note: The existing `StampContext` struct (pipeline.rs:14-19) has `work_item`, `evolver_item_id`, `log_summary`, `prompt`. The pure function should build the prompt without I/O. The actual `build_evolve_prompt` takes 7 args:

```rust
// Actual signature (evolution/prompts.rs:7-14):
// build_evolve_prompt(dimension, score, comment: Option<&str>,
//     work_item_title, work_item_id, log_summary, existing_skills)

// stamp::Model fields: id, target_rig, work_item_id, dimension, score,
//     severity, stamped_by, comment: Option<String>, evolved_at, active_skill_versions, timestamp
// Note: stamp does NOT have work_item_title — that comes from the WorkItem via board.get()

pub(crate) struct PreparedPrompt {
    pub prompt: String,
    pub existing_skill_pairs: Vec<(String, String)>,
}

/// Pure function — no I/O. work_item_title comes from caller (board.get()).
pub(crate) fn build_evolve_prompt_pure(
    dimension: &str,
    score: f32,
    comment: Option<&str>,
    work_item_title: &str,
    work_item_id: i64,
    log_summary: &str,
    skills: &[LoadedSkill],
) -> PreparedPrompt {
    let existing_skill_pairs = build_existing_skill_pairs(skills);
    let prompt = evolve::build_evolve_prompt(
        dimension, score, comment,
        work_item_title, work_item_id, log_summary,
        &existing_skill_pairs,
    );
    PreparedPrompt { prompt, existing_skill_pairs }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p opengoose --lib evolver::pipeline::tests::build_context -v`
Expected: 3 PASS

- [ ] **Step 5: Extract pure response parser**

Add tests first:

```rust
#[test]
fn validate_and_parse_skip_response() {
    let raw = "SKIP: not relevant";
    let parsed = validate_and_parse_response(raw);
    assert!(matches!(parsed, Ok(ParsedAction::Skip(_))));
}

#[test]
fn validate_and_parse_invalid_response() {
    let raw = "this is garbage";
    let parsed = validate_and_parse_response(raw);
    assert!(parsed.is_err());
}
```

Then extract `validate_and_parse_response(raw: &str) -> Result<ParsedAction>` from `execute_action()` — the parsing/validation logic that currently sits between the LLM call and the board state changes.

- [ ] **Step 6: Run all pipeline tests**

Run: `cargo test -p opengoose --lib evolver::pipeline -v`
Expected: All pass

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose/src/evolver/pipeline.rs
git commit -m "refactor(pipeline): extract pure context builder + response parser from I/O"
```

---

## Task 2: Extract pure metadata/name functions from skills writer

**Files:**
- Modify: `crates/opengoose-skills/src/evolution/writer/mod.rs:33-119`

- [ ] **Step 1: Write tests for pure extractors**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_name_extracts_from_frontmatter() {
        let content = "---\nname: auto-commit\ndescription: foo\n---\nbody";
        assert_eq!(parse_skill_name(content).unwrap(), "auto-commit");
    }

    #[test]
    fn parse_skill_name_fails_on_missing_name() {
        let content = "---\ndescription: foo\n---\nbody";
        assert!(parse_skill_name(content).is_err());
    }

    #[test]
    fn compute_version_bump_increments() {
        assert_eq!(compute_version_bump(Some(3)), 4);
    }

    #[test]
    fn compute_version_bump_defaults_to_one_when_none() {
        assert_eq!(compute_version_bump(None), 1);
    }

    #[test]
    fn build_skill_metadata_sets_all_fields() {
        // Actual SkillMetadata fields: generated_from: GeneratedFrom, generated_at: String,
        // evolver_work_item_id: Option<i64>, last_included_at: Option<String>,
        // effectiveness: Effectiveness { injected_count, subsequent_scores },
        // skill_version: u32
        let meta = build_skill_metadata(42, 10, "quality", 4.5, 2, None);
        assert_eq!(meta.skill_version, 2);
        assert_eq!(meta.generated_from.stamp_id, 42);
        assert_eq!(meta.generated_from.work_item_id, 10);
        assert_eq!(meta.generated_from.dimension, "quality");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p opengoose-skills --lib evolution::writer::tests -- 2>&1 | head -10`
Expected: FAIL — functions not found

- [ ] **Step 3: Implement pure functions**

```rust
pub(crate) fn parse_skill_name(content: &str) -> anyhow::Result<String> {
    parse_skill_header(content)
        .ok_or_else(|| anyhow::anyhow!("no name found in skill content"))
}

pub(crate) fn compute_version_bump(existing_version: Option<u32>) -> u32 {
    existing_version.map_or(1, |v| v + 1)
}

/// Pure metadata builder. Actual struct fields:
/// SkillMetadata { generated_from: GeneratedFrom, generated_at, evolver_work_item_id,
///   last_included_at, effectiveness: Effectiveness { injected_count, subsequent_scores }, skill_version }
pub(crate) fn build_skill_metadata(
    stamp_id: i64,
    work_item_id: i64,
    dimension: &str,
    score: f32,
    version: u32,
    evolver_work_item_id: Option<i64>,
) -> SkillMetadata {
    SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id,
            work_item_id,
            dimension: dimension.to_string(),
            score,
        },
        generated_at: Utc::now().to_rfc3339(),
        evolver_work_item_id,
        last_included_at: None,
        effectiveness: Effectiveness {
            injected_count: 0,
            subsequent_scores: Vec::new(),
        },
        skill_version: version,
    }
}
```

Then refactor `write_skill_to_rig_scope()` and `update_existing_skill()` to call these pure functions and only do file I/O themselves.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p opengoose-skills --lib evolution::writer -v`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-skills/src/evolution/writer/mod.rs
git commit -m "refactor(skills/writer): extract pure metadata builder + name parser from I/O"
```

---

## Task 3: Refactor scan_scope to return instead of mutate

**Files:**
- Modify: `crates/opengoose-skills/src/loader.rs:37-136`

- [ ] **Step 1: Write test for pure catalog builder**

Note: `SkillScope` has only `Installed` and `Learned` variants. The rig>project>global priority is determined by scan ORDER in `load_skills_inner`, not by enum. The `build_catalog` function takes skills already ordered by priority (rig first, then project, then global) and deduplicates by name (first seen wins).

```rust
#[test]
fn build_catalog_first_scope_wins_on_duplicate_name() {
    // Rig-scoped skill scanned first → wins over global with same name
    let rig_skill = make_loaded_skill("my-skill", "/rigs/r1/skills/learned/my-skill", SkillScope::Learned);
    let global_skill = make_loaded_skill("my-skill", "/global/skills/learned/my-skill", SkillScope::Learned);
    let catalog = build_catalog(vec![rig_skill, global_skill]);
    assert_eq!(catalog.len(), 1);
    assert!(catalog[0].path.to_str().unwrap().contains("/rigs/"));
}

#[test]
fn build_catalog_keeps_distinct_names() {
    let s1 = make_loaded_skill("a", "/path/a", SkillScope::Learned);
    let s2 = make_loaded_skill("b", "/path/b", SkillScope::Installed);
    let catalog = build_catalog(vec![s1, s2]);
    assert_eq!(catalog.len(), 2);
}

#[test]
fn build_catalog_empty_input_returns_empty() {
    let catalog = build_catalog(vec![]);
    assert!(catalog.is_empty());
}

// Helper
fn make_loaded_skill(name: &str, path: &str, scope: SkillScope) -> LoadedSkill {
    LoadedSkill {
        name: name.to_string(),
        description: format!("Test: {name}"),
        path: PathBuf::from(path),
        content: format!("---\nname: {name}\n---\nbody"),
        scope,
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p opengoose-skills --lib loader::tests::build_catalog -v`
Expected: FAIL

- [ ] **Step 3: Implement return-based scan_scope + pure catalog builder**

Change `scan_scope` signature from:
```rust
fn scan_scope(dir: &Path, scope: SkillScope, skills: &mut Vec<LoadedSkill>, seen: &mut HashSet<String>)
```
to:
```rust
fn scan_scope(dir: &Path, scope: SkillScope) -> Vec<LoadedSkill>
```

Add pure catalog builder that deduplicates by name (first occurrence wins — caller controls priority by ordering):
```rust
fn build_catalog(ordered_skills: Vec<LoadedSkill>) -> Vec<LoadedSkill> {
    let mut seen = HashSet::new();
    ordered_skills
        .into_iter()
        .filter(|s| seen.insert(s.name.clone()))
        .collect()
}
```

Update `load_skills_inner` to compose: scan rig scope, then project, then global → concatenate → build_catalog.

- [ ] **Step 4: Run all loader tests**

Run: `cargo test -p opengoose-skills --lib loader -v`
Expected: All pass (existing + new)

- [ ] **Step 5: Run workspace tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass (callers of load_skills unaffected — public API unchanged)

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-skills/src/loader.rs
git commit -m "refactor(loader): return-based scan_scope + pure catalog builder"
```

---

## Task 4: Fix CowStore false mutation pattern

**Files:**
- Modify: `crates/opengoose-board/src/store/mod.rs:78-86`

- [ ] **Step 1: Write test for &self branch**

```rust
#[test]
fn branch_from_shared_ref() {
    let store = seeded_store();
    // This should compile with &self, not &mut self
    let branch = store.branch(&RigId::new("test"));
    assert_eq!(branch.snapshot().len(), store.main.len());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opengoose-board --lib store::tests::branch_from_shared -v`
Expected: FAIL — cannot borrow `store` as mutable (current signature requires &mut self)

- [ ] **Step 3: Change branch() to &self, remove discard()**

```rust
// Before (lines 78-86):
pub fn branch(&mut self, rig_id: &RigId) -> Branch { ... }
pub fn discard(&mut self, branch: Branch) { let _ = branch; }

// After:
pub fn branch(&self, rig_id: &RigId) -> Branch {
    let base_commit = self.commits.last().map(|c| c.id.0).unwrap_or(0);
    Branch::new(rig_id.clone(), Arc::clone(&self.main), base_commit)
}
// discard() removed — callers use drop(branch)
```

- [ ] **Step 4: Fix all callers of discard()**

Search for `.discard(` across workspace and replace with `drop()`:
```
crates/opengoose-board/src/board.rs — board.store.discard(branch) → drop(branch)
```

- [ ] **Step 5: Fix all callers that used &mut for branch()**

Any caller that borrowed `&mut store` just to call `branch()` can now use `&store`.

- [ ] **Step 6: Run all tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose-board/src/store/mod.rs crates/opengoose-board/src/board.rs
git commit -m "refactor(CowStore): branch() takes &self, remove no-op discard()"
```

---

## Task 5: Fix silent error drops in worker.rs

**Files:**
- Modify: `crates/opengoose-rig/src/rig/worker.rs:109,124,135,222,231,266`

- [ ] **Step 1: Replace .ok() with warn!() logging**

For each `.ok()` call in worker.rs, replace with explicit error logging:

```rust
// Before (line 109):
board.abandon(item.id).await.ok();

// After:
if let Err(e) = board.abandon(item.id).await {
    warn!(error = %e, item_id = item.id, "failed to abandon work item after worktree failure");
}
```

Apply the same pattern to all 7 instances, with context-specific messages:
- Line 109: "after worktree acquisition failure"
- Line 124: "after middleware on_start failure"
- Line 135: "after session resolution failure"
- Line 222: "after LLM execution failure"
- Line 231: "after validation infrastructure failure"
- Line 266: mark_stuck — "after max retries exceeded"
- Line 283: `.ok()?` in find_session_by_name — this one is different (converts Result to Option); log at debug level

- [ ] **Step 2: Ensure tracing import exists**

Verify `use tracing::warn;` is in imports. Add if missing.

- [ ] **Step 3: Run tests**

Run: `cargo test -p opengoose-rig --lib rig -v`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/rig/worker.rs
git commit -m "fix(worker): log all silent error drops instead of .ok() swallowing"
```

---

## Task 6: Fix silent error drops across remaining files

**Files:**
- Modify: `crates/opengoose/src/tui/event/rigs.rs:26,31,41,44`
- Modify: `crates/opengoose/src/tui/tui_layer.rs:42`
- Modify: `crates/opengoose/src/web/mod.rs:27`
- Modify: `crates/opengoose/src/evolver/sweep.rs:137`
- Modify: `crates/opengoose/src/main.rs:41,734-766`
- Modify: Remaining ~23 instances across workspace

- [ ] **Step 1: Fix tui/event/rigs.rs (4 instances)**

```rust
// Before:
let _ = tx.send(msg);
// After:
if let Err(e) = tx.send(msg) {
    warn!(error = %e, "rigs event channel closed");
}
```

- [ ] **Step 2: Fix tui/tui_layer.rs (1 instance)**

```rust
// Before:
let _ = self.tx.try_send(entry);
// After:
if let Err(e) = self.tx.try_send(entry) {
    // Use eprintln since this IS the logging layer — can't log to itself
    eprintln!("tui log channel full or closed: {e}");
}
```

- [ ] **Step 3: Fix web/mod.rs (1 instance)**

```rust
if let Err(e) = tx2.send(()) {
    warn!(error = %e, "shutdown signal channel closed");
}
```

- [ ] **Step 4: Fix evolver/sweep.rs:137**

```rust
// Before:
let _ = apply_decision(decision, &dormant);
// After:
if let Err(e) = apply_decision(decision, &dormant) {
    warn!(error = %e, skill = dormant.name, "sweep decision apply failed");
}
```

- [ ] **Step 5: Fix main.rs silent drops**

Line 41: `std::fs::create_dir_all(&dir).ok()` → log on error
Lines 734-766: board ops → warn on error

- [ ] **Step 6: Audit and fix remaining ~23 instances**

For each remaining `.ok()` / `let _ =` in non-test code, apply the appropriate pattern:
- Filesystem fallbacks (metadata.rs, retention.rs, discover/parse.rs): log at `debug!()` level
- Channel sends (web/sse.rs): log at `warn!()` level
- Path operations (worktree.rs, web/api/skills.rs): log at `debug!()` level

- [ ] **Step 7: Verify zero silent drops remain**

Run: `grep -rn '\.ok()' crates/*/src/**/*.rs | grep -v '#\[cfg(test)\]' | grep -v 'mod tests'` — manually verify each remaining `.ok()` is justified and logged.

- [ ] **Step 8: Run workspace tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "fix: replace all silent error drops with warn!/debug! logging"
```

---

## Task 7: Create crate-specific error types

**Files:**
- Create: `crates/opengoose-skills/src/error.rs`
- Create: `crates/opengoose-rig/src/error.rs`
- Modify: `crates/opengoose-skills/src/lib.rs`
- Modify: `crates/opengoose-rig/src/lib.rs`

- [ ] **Step 1: Define SkillError**

```rust
// crates/opengoose-skills/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("skill load failed: {0}")]
    LoadFailed(String),
    #[error("invalid frontmatter: {0}")]
    InvalidFrontmatter(String),
    #[error("evolution failed: {0}")]
    EvolutionFailed(String),
    #[error("skill not found: {0}")]
    NotFound(String),
    #[error("filesystem error: {0}")]
    Fs(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

- [ ] **Step 2: Define RigError**

```rust
// crates/opengoose-rig/src/error.rs
use thiserror::Error;
use opengoose_board::BoardError;

#[derive(Debug, Error)]
pub enum RigError {
    #[error("session failed: {0}")]
    SessionFailed(String),
    #[error("worktree failed: {0}")]
    WorktreeFailed(String),
    #[error("board error: {0}")]
    Board(#[from] BoardError),
    #[error("middleware error: {0}")]
    Middleware(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

- [ ] **Step 3: Wire into lib.rs**

Add `pub mod error;` and `pub use error::*;` to each crate's lib.rs.

- [ ] **Step 4: Migrate key functions**

Start with the most-called functions in each crate. Convert return types from `anyhow::Result<T>` to `Result<T, SkillError>` / `Result<T, RigError>`. Add `From` impls as needed.

Do NOT migrate every function at once — start with public API surfaces (loader, writer, worker).

- [ ] **Step 5: Run workspace tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-skills/src/error.rs crates/opengoose-rig/src/error.rs \
       crates/opengoose-skills/src/lib.rs crates/opengoose-rig/src/lib.rs
git commit -m "refactor: add SkillError + RigError crate-specific error types"
```

---

## Task 8: Create test fixtures

**Files:**
- Create: `crates/opengoose-board/src/test_fixtures.rs`
- Create: `crates/opengoose-skills/src/test_fixtures.rs`
- Create: `crates/opengoose-rig/src/test_fixtures.rs`

- [ ] **Step 1: Board fixtures**

Check existing helpers first — `store/mod.rs:115` has `make_item`, `merge.rs:313` has `make_item`, `board/mod.rs` has `new_board()`. Consolidate into one place.

```rust
// crates/opengoose-board/src/test_fixtures.rs
#![cfg(test)]

use crate::work_item::*;
use crate::Board;

/// Actual WorkItem fields: id, title, description, created_by: RigId,
/// created_at, status, priority, tags, claimed_by: Option<RigId>, updated_at
pub fn make_work_item(id: i64, status: Status, priority: Priority) -> WorkItem {
    WorkItem {
        id,
        title: format!("Test item {id}"),
        description: String::new(),
        created_by: RigId::new("test"),
        created_at: chrono::Utc::now(),
        status,
        priority,
        tags: vec![],
        claimed_by: None,
        updated_at: chrono::Utc::now(),
    }
}

pub async fn in_memory_board() -> Board {
    Board::in_memory().await.expect("in-memory board should connect")
}
```

- [ ] **Step 2: Skills fixtures**

```rust
// crates/opengoose-skills/src/test_fixtures.rs
#![cfg(test)]

use crate::loader::{LoadedSkill, SkillScope};
use crate::metadata::*;
use std::path::PathBuf;

/// Actual LoadedSkill: name, description, path: PathBuf, content: String, scope: SkillScope
pub fn make_skill(name: &str, scope: SkillScope) -> LoadedSkill {
    LoadedSkill {
        name: name.to_string(),
        description: format!("Test skill: {name}"),
        path: PathBuf::from(format!("/tmp/test-skills/{name}")),
        content: format!("---\nname: {name}\n---\nTest body"),
        scope,
    }
}

/// Actual SkillMetadata: generated_from, generated_at, evolver_work_item_id,
/// last_included_at, effectiveness: Effectiveness { injected_count, subsequent_scores }, skill_version
pub fn make_metadata(version: u32) -> SkillMetadata {
    SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id: 1,
            work_item_id: 1,
            dimension: "quality".to_string(),
            score: 3.0,
        },
        generated_at: chrono::Utc::now().to_rfc3339(),
        evolver_work_item_id: None,
        last_included_at: None,
        effectiveness: Effectiveness {
            injected_count: 0,
            subsequent_scores: vec![],
        },
        skill_version: version,
    }
}
```

- [ ] **Step 3: Rig fixtures**

```rust
// crates/opengoose-rig/src/test_fixtures.rs
#![cfg(test)]

pub fn temp_home() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp home");
    std::env::set_var("HOME", dir.path());
    dir
}
```

- [ ] **Step 4: Wire modules into lib.rs/mod.rs**

Add `#[cfg(test)] mod test_fixtures;` to each crate.

- [ ] **Step 5: Migrate one existing test to use fixtures**

Pick one test per crate that manually builds items and convert to use the new fixtures. Verify it passes.

- [ ] **Step 6: Commit**

```bash
git add crates/*/src/test_fixtures.rs crates/*/src/lib.rs
git commit -m "test: add shared test fixtures for board, skills, and rig crates"
```

---

## Task 9: Test stamp_ops.rs (10 untested pub functions)

**Files:**
- Modify: `crates/opengoose-board/src/stamp_ops.rs`

**Note on Board API:** `board.post()` takes `PostWorkItem` struct (check exact fields). `register_rig()` takes `(id, rig_type, recipe, tags)`. Verify exact signatures before writing tests — use `cargo doc -p opengoose-board --open` or read the source.

- [ ] **Step 1: Write tests for stamp query functions**

The implementer should read `stamp_ops.rs`, `rigs.rs`, and `work_items/transitions.rs` to get exact method signatures, then write tests that:
- Create an in-memory board via `Board::in_memory().await`
- Post a work item, register a rig, add stamps
- Test each of the 10 pub functions with both happy path and edge cases

Key test scenarios:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::*;

    #[tokio::test]
    async fn stamps_for_item_returns_empty_when_no_stamps() {
        let board = in_memory_board().await;
        let stamps = board.stamps_for_item(999).await.expect("query");
        assert!(stamps.is_empty());
    }

    #[tokio::test]
    async fn weighted_score_returns_zero_for_unknown_rig() {
        let board = in_memory_board().await;
        let score = board.weighted_score("nonexistent").await.expect("query");
        assert_eq!(score, 0.0);
    }

    // add_stamp → stamps_for_item roundtrip
    // add_stamp → weighted_score reflects score
    // add_stamp → stamps_for_rig returns stamp
    // batch_rig_scores with multiple rigs
    // mark_stamp_evolved sets evolved_at
    // unprocessed_low_stamps filters by threshold
    // recent_low_stamps filters by days + threshold
    // trust_level returns appropriate level for new rig
    // stamps_with_scores returns tuple of (stamps, dimension_scores, weighted)
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p opengoose-board --lib stamp_ops -v`
Expected: All pass

- [ ] **Step 3: Add proptest for weighted_score monotonicity**

```rust
#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn weighted_score_is_non_negative(scores in prop::collection::vec(0.0f32..5.0, 0..10)) {
            // weighted_score should always be >= 0
            let total: f32 = scores.iter().sum();
            let count = scores.len() as f32;
            if count > 0.0 {
                prop_assert!((total / count) >= 0.0);
            }
        }
    }
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-board/src/stamp_ops.rs
git commit -m "test(stamp_ops): add unit tests for all 10 pub functions"
```

---

## Task 10: Test rigs.rs and queries.rs

**Files:**
- Modify: `crates/opengoose-board/src/rigs.rs`
- Modify: `crates/opengoose-board/src/work_items/queries.rs`

- [ ] **Step 1: Write rigs.rs tests**

```rust
#[cfg(test)]
mod tests {
    use crate::test_fixtures::*;

**Note:** `register_rig` takes `(id, rig_type, recipe, tags)` — verify exact signature.

Key test scenarios:
```rust
    // register_rig + list_rigs roundtrip (verify count)
    // get_rig returns None for unknown
    // get_rig returns Some for registered
    // remove_rig + verify list is empty
```
}
```

- [ ] **Step 2: Write queries.rs tests**

```rust
#[cfg(test)]
mod tests {
    use crate::test_fixtures::*;

**Note:** `board.post()` takes `PostWorkItem` struct — verify exact fields. `board.claim()` takes `(item_id, &RigId)`.

Key test scenarios:
```rust
    // get(999) returns None for missing
    // post → list returns posted items
    // post → ready() → ready_items() returns ready items
    // post → ready → claim → claimed_by returns claimed items
    // completed_by_rig returns only submitted items
```
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p opengoose-board -v 2>&1 | tail -20`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-board/src/rigs.rs crates/opengoose-board/src/work_items/queries.rs
git commit -m "test(board): add unit tests for rigs.rs and queries.rs"
```

---

## Task 11: Test work_mode.rs (verify coverage, add missing)

**Files:**
- Modify: `crates/opengoose-rig/src/work_mode.rs`

**Note:** work_mode.rs may already have tests. Check existing tests first (`cargo test -p opengoose-rig --lib work_mode -- --list`). Only add tests for genuinely untested paths.

**WorkInput fields:** `text: String`, `work_id: Option<i64>`, `session_id: Option<String>` (NOT `content`)

- [ ] **Step 1: Check existing test coverage**

Run: `cargo test -p opengoose-rig --lib work_mode -- --list 2>&1`
Review which scenarios are already covered.

- [ ] **Step 2: Add only missing tests**

Focus on edge cases not covered by existing tests. Example patterns using correct field names:

```rust
#[test]
fn evolve_mode_uses_work_id() {
    let mode = EvolveMode;
    let input = WorkInput { text: String::new(), work_id: Some(42), session_id: None };
    let session = mode.session_for(&input);
    assert!(session.contains("42"));
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p opengoose-rig --lib work_mode -v`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose-rig/src/work_mode.rs
git commit -m "test(work_mode): add unit tests for ChatMode, TaskMode, EvolveMode"
```

---

## Task 12: Proptest expansion

**Files:**
- Modify: `crates/opengoose-board/src/merge.rs` (roundtrip + associativity)
- Modify: `crates/opengoose-board/src/work_item.rs` (serde roundtrip)
- Modify: `crates/opengoose-skills/src/metadata.rs` (JSON roundtrip)
- Modify: `crates/opengoose-board/src/work_items/transitions.rs` (terminal states)

**Note:** All proptests need `arb_*` strategy functions. Check if `merge_props.rs` already defines `arb_work_item()` — if so, reuse it. If not, define it. SkillMetadata does NOT derive `Default` — build it explicitly.

- [ ] **Step 1: Add merge associativity proptest**

In `crates/opengoose-board/tests/merge_props.rs`, check if `arb_work_item()` strategy exists. If not, create one that generates arbitrary WorkItem values. Then:

```rust
proptest! {
    #[test]
    fn merge_is_associative(
        a in arb_work_item(),
        b in arb_work_item(),
        c in arb_work_item(),
    ) {
        let ab_c = a.merge(&b).merge(&c);
        let a_bc = a.merge(&b.merge(&c));
        prop_assert_eq!(ab_c.status, a_bc.status);
        prop_assert_eq!(ab_c.priority, a_bc.priority);
        prop_assert_eq!(ab_c.tags, a_bc.tags);
    }
}
```

- [ ] **Step 2: Add WorkItem serde roundtrip proptest**

```rust
proptest! {
    #[test]
    fn work_item_serde_roundtrip(item in arb_work_item()) {
        let json = serde_json::to_string(&item).expect("serialize");
        let back: WorkItem = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(item.id, back.id);
        prop_assert_eq!(item.status, back.status);
        prop_assert_eq!(item.priority, back.priority);
    }
}
```

- [ ] **Step 3: Add SkillMetadata JSON roundtrip proptest**

In `crates/opengoose-skills/src/metadata.rs`. Build `SkillMetadata` explicitly (no Default):

```rust
fn arb_metadata() -> impl Strategy<Value = SkillMetadata> {
    (any::<u32>(), "\\w{1,20}", 0i64..1000, 0.0f32..5.0)
        .prop_map(|(version, dim, stamp_id, score)| SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id, work_item_id: 1,
                dimension: dim, score,
            },
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness { injected_count: 0, subsequent_scores: vec![] },
            skill_version: version,
        })
}
```

- [ ] **Step 4: Add terminal state proptest + conversation_log retention proptest**

Terminal states:
```rust
proptest! {
    #[test]
    fn terminal_states_reject_all_transitions(
        target in prop_oneof![Just(Status::Done), Just(Status::Abandoned)],
        next in arb_status(),
    ) {
        prop_assert!(validate_transition(target, next).is_err());
    }
}
```

Conversation log retention (spec 3d):
```rust
// In conversation_log tests — proptest that retention policy never deletes
// files that should be kept (files_to_keep ⊆ all_files)
```

- [ ] **Step 5: Run all proptests**

Run: `cargo test --workspace proptest -v 2>&1 | tail -20`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-board/tests/ crates/opengoose-board/src/ crates/opengoose-skills/src/metadata.rs
git commit -m "test: expand proptest coverage — merge associativity, serde roundtrips, terminal states"
```

---

## Task 13: Error path tests

**Files:**
- Modify: Various test modules across all crates

- [ ] **Step 1: Board error path tests**

```rust
// In transitions test module
#[tokio::test]
async fn claim_already_claimed_item_fails() {
    let board = in_memory_board().await;
    let id = board.post("A", "", "medium", "normal", &[]).await.expect("post");
    board.ready(id).await.expect("ready");
    board.register_rig("rig-1").await.expect("reg");
    board.register_rig("rig-2").await.expect("reg");
    board.claim(id, "rig-1").await.expect("first claim");
    let result = board.claim(id, "rig-2").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn transition_from_done_fails() {
    let board = in_memory_board().await;
    let id = board.post("A", "", "medium", "normal", &[]).await.expect("post");
    board.ready(id).await.expect("ready");
    board.register_rig("rig-1").await.expect("reg");
    board.claim(id, "rig-1").await.expect("claim");
    board.submit(id, "rig-1").await.expect("submit");
    // Item is now Done — should reject further transitions
    let result = board.claim(id, "rig-1").await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Skills error path tests**

```rust
// In loader test module
#[test]
fn load_skills_from_nonexistent_dir_returns_empty() {
    let skills = load_skills(Path::new("/nonexistent"), None, None);
    assert!(skills.is_empty());
}

// In evolution/writer test module
#[test]
fn parse_skill_name_with_empty_content_fails() {
    assert!(parse_skill_name("").is_err());
}

#[test]
fn parse_skill_name_with_no_frontmatter_fails() {
    assert!(parse_skill_name("just some text").is_err());
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "test: add error path tests for board transitions, skills loading, and writer"
```

---

## Task 13b: Upgrade weak .is_ok()-only assertions (spec 3c)

**Files:**
- Modify: Various test modules across all crates

- [ ] **Step 1: Find all weak assertions**

Run: `grep -rn '\.is_ok()' crates/*/src/ crates/*/tests/ | grep 'assert'` to find all `assert!(x.is_ok())` patterns.

- [ ] **Step 2: Upgrade board stamp_ops tests**

Replace `.is_ok()` checks with actual value assertions — verify scores, counts, timestamps.

- [ ] **Step 3: Upgrade web API tests**

Add response body validation (not just status 200).

- [ ] **Step 4: Upgrade skills loader tests**

Verify loaded skill contents match expected values.

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "test: upgrade 34 weak .is_ok() assertions to value checks"
```

---

## Task 13c: Remove unnecessary mut + fold/collect conversion (spec 4d)

**Files:**
- Modify: Various files across all crates

- [ ] **Step 1: Find unnecessary mut bindings**

Run: `cargo clippy --workspace -- -W clippy::needless_pass_by_ref_mut -W clippy::unnecessary_mut_passed`

- [ ] **Step 2: Convert accumulation loops to iterator chains**

Look for patterns like:
```rust
let mut result = Vec::new();
for item in items {
    if condition(item) {
        result.push(transform(item));
    }
}
```
Convert to: `items.iter().filter(condition).map(transform).collect()`

Only convert where readability improves.

- [ ] **Step 3: Run tests + clippy**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings`

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: remove unnecessary mut, convert loops to iterator chains"
```

---

## Task 14: RigId validation + SkillName newtype

**Files:**
- Modify: `crates/opengoose-board/src/work_item.rs:20-22`
- Create: `crates/opengoose-skills/src/skill_name.rs`

- [ ] **Step 1: Write RigId validation tests**

```rust
#[test]
fn rig_id_rejects_empty() {
    assert!(RigId::try_new("").is_err());
}

#[test]
fn rig_id_rejects_path_traversal() {
    assert!(RigId::try_new("../etc").is_err());
    assert!(RigId::try_new("foo/bar").is_err());
    assert!(RigId::try_new("foo\\bar").is_err());
}

#[test]
fn rig_id_accepts_valid() {
    assert!(RigId::try_new("operator").is_ok());
    assert!(RigId::try_new("rig-123").is_ok());
}
```

- [ ] **Step 2: Add validation to RigId**

```rust
impl RigId {
    pub fn try_new(id: impl Into<String>) -> anyhow::Result<Self> {
        let id = id.into();
        if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
            anyhow::bail!("invalid rig id: {id:?}");
        }
        Ok(Self(id))
    }

    // Keep new() for trusted internal use, but add a note
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}
```

- [ ] **Step 3: Create SkillName newtype**

```rust
// crates/opengoose-skills/src/skill_name.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SkillName(String);

impl SkillName {
    pub fn try_new(name: impl Into<String>) -> anyhow::Result<Self> {
        let name = name.into();
        if name.is_empty() {
            anyhow::bail!("skill name cannot be empty");
        }
        if name.len() > 64 {
            anyhow::bail!("skill name too long: {} chars", name.len());
        }
        if name.contains('/') || name.contains("..") {
            anyhow::bail!("skill name contains invalid chars: {name:?}");
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SkillName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
```

- [ ] **Step 4: Wire into crate + migrate key call sites**

Add `pub mod skill_name;` to skills lib.rs. Start using `SkillName` in `LoadedSkill.name` and `SkillMetadata.name`. Migrate incrementally — don't change every reference at once.

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-board/src/work_item.rs crates/opengoose-skills/src/skill_name.rs \
       crates/opengoose-skills/src/lib.rs
git commit -m "refactor: add validated RigId::try_new() + SkillName newtype"
```

---

## Task 15: Keys.rs command dispatch table

**Files:**
- Modify: `crates/opengoose/src/tui/event/keys.rs`

- [ ] **Step 1: Write test for individual handler**

```rust
#[tokio::test]
async fn handle_quit_returns_true() {
    let (app, tx, board, operator) = make_test_context().await;
    let result = handle_quit(&mut app, &tx, &board, &operator).await;
    assert!(result);
}
```

- [ ] **Step 2: Define handler type + dispatch table**

```rust
type KeyHandler = for<'a> fn(
    &'a mut App,
    &'a mpsc::Sender<AgentMsg>,
    &'a Arc<Board>,
    &'a Arc<Operator>,
) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

fn build_global_keymap() -> HashMap<(KeyCode, KeyModifiers), KeyHandler> {
    let mut map = HashMap::new();
    map.insert((KeyCode::Char('c'), KeyModifiers::CONTROL), |app, _, _, _| Box::pin(async { true }) as _);
    // ... etc
    map
}
```

- [ ] **Step 3: Refactor handle_key to use dispatch table**

Replace the match statement with table lookup:

```rust
pub async fn handle_key(key: KeyEvent, app: &mut App, ...) -> bool {
    let global_map = build_global_keymap();
    if let Some(handler) = global_map.get(&(key.code, key.modifiers)) {
        return handler(app, agent_tx, board, operator).await;
    }
    // Tab-specific dispatch
    match app.active_tab {
        Tab::Chat => handle_chat_key(key, app, agent_tx, board, operator).await,
        Tab::Logs => handle_logs_key(key, app),
        _ => false,
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p opengoose --lib tui::event::keys -v`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/tui/event/keys.rs
git commit -m "refactor(keys): replace match dispatch with command table"
```

---

## Task 16: Clone minimization

**Files:**
- Modify: `crates/opengoose-rig/src/worktree.rs:34-38`
- Modify: `crates/opengoose-rig/src/mcp_tools/handlers.rs`
- Modify: `crates/opengoose/src/web/api/skills.rs`

- [ ] **Step 1: Fix worktree.rs destructure**

```rust
// Before (lines 34-38):
pub async fn remove(mut self) {
    self.keep = true;
    let repo = self.repo_dir.clone();
    let path = self.path.clone();
    let branch = self.branch.clone();
    tokio::task::spawn_blocking(move || remove_worktree(&repo, &path, &branch)).await;
}

// After:
pub async fn remove(self) {
    let Self { repo_dir, path, branch, .. } = self;
    tokio::task::spawn_blocking(move || remove_worktree(&repo_dir, &path, &branch)).await;
}
```

- [ ] **Step 2: Fix mcp_tools/handlers.rs rig_id clones**

Use `Arc<RigId>` or pass by reference where possible instead of cloning String on every call.

- [ ] **Step 3: Fix web/api/skills.rs metadata clones**

Build response structs from references instead of cloning all fields.

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose-rig/src/worktree.rs crates/opengoose-rig/src/mcp_tools/handlers.rs \
       crates/opengoose/src/web/api/skills.rs
git commit -m "refactor: minimize unnecessary clones in worktree, handlers, skills API"
```

---

## Task 17: Extract CLI from main.rs

**Files:**
- Create: `crates/opengoose/src/cli/mod.rs`
- Create: `crates/opengoose/src/cli/commands.rs`
- Create: `crates/opengoose/src/cli/setup.rs`
- Modify: `crates/opengoose/src/main.rs`

- [ ] **Step 1: Create cli module structure**

```rust
// crates/opengoose/src/cli/mod.rs
pub mod commands;
pub mod setup;
```

- [ ] **Step 2: Move subcommand handlers to commands.rs**

Extract from main.rs lines 56-72 — the Board, Rigs, Skills, Logs, Run match arms — into standalone async functions in `cli/commands.rs`.

- [ ] **Step 3: Move setup logic to setup.rs**

Extract `db_url()`, `home_dir()`, agent creation functions into `cli/setup.rs`.

- [ ] **Step 4: Thin out main.rs**

main.rs should contain only:
- CLI arg definitions (clap)
- `#[tokio::main] async fn main()` that parses args and delegates to cli::commands

Target: ~150 lines.

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose/src/cli/ crates/opengoose/src/main.rs
git commit -m "refactor(main): extract CLI commands + setup into cli/ module"
```

---

## Task 20: Final verification + clippy

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All pass, 80+ new tests added

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

- [ ] **Step 3: Verify completion criteria**

Check each metric from the spec:
- `cargo test --workspace` — green
- `cargo clippy --workspace` — warning-free
- Silent error drops — grep for remaining `.ok()` and `let _ =` in non-test code
- New test count — compare test counts before/after
- Type-safe newtypes — RigId validated, SkillName exists

- [ ] **Step 4: Commit any final fixes**

```bash
git add -A
git commit -m "chore: final clippy fixes and verification"
```
