# FP Quality Sweep v2 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Comprehensive code quality sweep — unwrap removal, pure function extraction, proptest introduction, test code quality — across all 4 crates in a single PR.

**Architecture:** Top-down impact-first approach. Each high-density file gets unwrap removal + pure function extraction + tests in one pass. Then proptest for CRDT/state machine/parsers. Finally mop-up remaining unwraps and test code quality.

**Tech Stack:** Rust, anyhow (`.context()`), proptest, thiserror (existing BoardError)

**Spec:** `docs/superpowers/specs/2026-03-24-fp-quality-sweep-design.md`

---

## File Map

### New files
- `crates/opengoose-board/Cargo.toml` — add `proptest` dev-dependency
- `crates/opengoose-board/src/merge_props.rs` — proptest module for CRDT properties
- `crates/opengoose-board/src/work_item_props.rs` — proptest module for state transitions

### Modified files
- `crates/opengoose/src/web/api/skills.rs` — unwrap removal, pure function extraction
- `crates/opengoose-skills/src/manage/promote.rs` — unwrap removal, pure function extraction
- `crates/opengoose-skills/src/manage/add.rs` — unwrap removal, pure function extraction
- `crates/opengoose/src/evolver/sweep.rs` — unwrap removal, pure function extraction
- `crates/opengoose/src/evolver/pipeline.rs` — unwrap removal, pure function extraction
- `crates/opengoose-board/src/merge.rs` — add proptest imports
- `crates/opengoose-board/src/lib.rs` — register new proptest modules
- Remaining mid-density files (Phase 3, determined dynamically)
- All test files (Phase 4, `.unwrap()` → `.expect()`)

---

## Task 1: web/api/skills.rs — Pure function extraction + unwrap removal

**Files:**
- Modify: `crates/opengoose/src/web/api/skills.rs`

- [ ] **Step 1: Extract `dedup_skills_by_name` pure function**

In `collect_all_skills()`, rig skills override global/project skills via HashMap insert (last-writer-wins). Extract this merge logic as a pure function that takes two skill lists and produces the merged HashMap:

```rust
fn merge_skill_sources(
    base: Vec<LoadedSkill>,
    overrides: Vec<LoadedSkill>,
) -> Vec<LoadedSkill> {
    let mut map: std::collections::HashMap<String, LoadedSkill> =
        base.into_iter().map(|s| (s.name.clone(), s)).collect();
    for skill in overrides {
        map.insert(skill.name.clone(), skill); // last-writer-wins: override takes priority
    }
    map.into_values().collect()
}
```

Update `collect_all_skills()` to collect global/project skills as `base`, rig skills as `overrides`, then call `merge_skill_sources(base, overrides)`.

- [ ] **Step 2: Extract `classify_scope` pure function**

Currently `determine_scope_level` mixes path comparison with scope classification. Extract the pure classification:

```rust
fn classify_scope(
    canon_path: &Path,
    canon_rigs: Option<&Path>,
    project_dir: Option<&Path>,
) -> String {
    if let Some(rigs) = canon_rigs {
        if let Ok(rel) = canon_path.strip_prefix(rigs) {
            if let Some(rig_id) = rel.components().next() {
                return format!("rig:{}", rig_id.as_os_str().to_string_lossy());
            }
        }
    }
    if let Some(pd) = project_dir {
        if canon_path.starts_with(pd) { return "project".into(); }
    }
    "global".into()
}
```

Update `determine_scope_level` to delegate to `classify_scope` after canonicalization.

- [ ] **Step 3: Add tests for extracted pure functions**

```rust
#[test]
fn merge_skill_sources_override_wins() { /* same name in both → override kept */ }

#[test]
fn merge_skill_sources_preserves_unique() { /* 3 unique across both → all kept */ }

#[test]
fn merge_skill_sources_empty_override() { /* empty overrides → base preserved */ }

#[test]
fn classify_scope_rig_path_includes_rig_id() { /* path under rig_dir → "rig:<id>" */ }

#[test]
fn classify_scope_project_path() { /* path under project_dir → "project" */ }

#[test]
fn classify_scope_global_fallback() { /* path elsewhere → "global" */ }

#[test]
fn classify_scope_no_dirs_returns_global() { /* both None → "global" */ }
```

- [ ] **Step 4: Convert test unwraps to expect**

All `.unwrap()` in the test module → `.expect("reason")` with context.

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p opengoose --filter-expr 'test(skills)'`
Expected: All existing + new tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose/src/web/api/skills.rs
git commit -m "refactor(web/api/skills): extract pure functions, remove unwraps, add tests"
```

---

## Task 2: manage/promote.rs — Pure function extraction + unwrap removal

**Files:**
- Modify: `crates/opengoose-skills/src/manage/promote.rs`

- [ ] **Step 1: Extract `resolve_target_path` pure function**

From `run()`, extract the target directory path construction:

```rust
fn resolve_target_path(base_dir: &Path, name: &str, to: &str) -> PathBuf {
    let dest_base = match to {
        "global" => base_dir.join("skills"),
        _ => base_dir.join("skills"), // same for now, extensible
    };
    dest_base.join(name)
}
```

- [ ] **Step 2: Extract `extract_rig_name` pure function**

From `run()` lines 53-58, the logic that extracts rig name from source path:

```rust
fn extract_rig_name(source: &Path) -> Option<String> {
    // source is .../rigs/<rig-id>/skills/learned/<name>
    // ancestors: learned/{name} → skills → {rig-id}
    source.ancestors().nth(3)
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
}
```

- [ ] **Step 3: Extract `build_promotion_metadata` pure function**

From `run()`, the JSON metadata update logic. The actual code uses guarded `if let` chains and adds `promoted_to` + `promoted_at` (timestamp):

```rust
fn apply_promotion_metadata(meta: &mut serde_json::Value, to: &str) -> bool {
    if let Some(obj) = meta.as_object_mut() {
        obj.insert("promoted_to".into(), serde_json::Value::String(to.to_string()));
        obj.insert("promoted_at".into(), serde_json::Value::String(chrono::Utc::now().to_rfc3339()));
        true
    } else {
        false
    }
}
```

The caller in `run()` still handles the filesystem guard (`meta_path.exists()`, `read_to_string`, `from_str`) — only the pure metadata mutation is extracted.

- [ ] **Step 4: Add tests for extracted pure functions**

```rust
#[test]
fn extract_rig_name_from_valid_path() { /* .../rigs/my-rig/skills/foo → Some("my-rig") */ }

#[test]
fn extract_rig_name_from_short_path() { /* too short → None */ }

#[test]
fn apply_promotion_metadata_sets_fields() { /* valid object → promoted_to + promoted_at set */ }

#[test]
fn apply_promotion_metadata_non_object_returns_false() { /* array value → false */ }
```

- [ ] **Step 5: Convert test unwraps to expect**

All `.unwrap()` in tests → `.expect("reason")`.

- [ ] **Step 6: Run tests**

Run: `cargo nextest run -p opengoose-skills --filter-expr 'test(promote)'`
Expected: All existing + new tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose-skills/src/manage/promote.rs
git commit -m "refactor(manage/promote): extract pure functions, remove unwraps, add tests"
```

---

## Task 3: manage/add.rs — Pure function extraction + unwrap removal

**Files:**
- Modify: `crates/opengoose-skills/src/manage/add.rs`

- [ ] **Step 1: Extract `filter_skills_by_name` pure function**

From `select_skills()`, extract the non-interactive filter logic:

```rust
fn filter_skills_by_name<'a>(
    skills: &'a [DiscoveredSkill],
    name: &str,
) -> Result<Vec<DiscoveredSkill>> {
    let matches: Vec<_> = skills.iter().filter(|s| s.name == name).cloned().collect();
    if matches.is_empty() {
        anyhow::bail!("skill '{}' not found in repository", name);
    }
    Ok(matches)
}
```

- [ ] **Step 2: Extract `is_copyable_entry` pure predicate**

From `copy_dir_recursive()`:

```rust
fn is_copyable_entry(name: &std::ffi::OsStr) -> bool {
    let s = name.to_string_lossy();
    // Rust's read_dir never yields "." or ".." — no need to check
    !s.starts_with('.') && s != "__pycache__"
}
```

- [ ] **Step 3: Add tests for extracted pure functions**

```rust
#[test]
fn filter_skills_by_name_finds_match() { /* skill present → returned */ }

#[test]
fn filter_skills_by_name_not_found_errors() { /* no match → error */ }

#[test]
fn is_copyable_entry_allows_normal_files() { /* "skill.yaml" → true */ }

#[test]
fn is_copyable_entry_blocks_git() { /* ".git" → false */ }

#[test]
fn is_copyable_entry_blocks_pycache() { /* "__pycache__" → false */ }
```

- [ ] **Step 4: Convert test unwraps to expect**

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p opengoose-skills --filter-expr 'test(add)'`
Expected: All existing + new tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-skills/src/manage/add.rs
git commit -m "refactor(manage/add): extract pure functions, remove unwraps, add tests"
```

---

## Task 4: evolver/sweep.rs — Pure function extraction + unwrap removal

**Files:**
- Modify: `crates/opengoose/src/evolver/sweep.rs`

- [ ] **Step 1: Extract `find_dormant_skill` pure lookup**

From `apply_decision()`, extract the skill lookup:

```rust
fn find_dormant_skill<'a>(dormant: &'a [LoadedSkill], name: &str) -> Option<&'a LoadedSkill> {
    dormant.iter().find(|s| s.name == name)
}
```

- [ ] **Step 2: Extract `format_effectiveness_entry` pure function**

From `build_effectiveness_summary()`, if there's per-skill formatting logic, extract it as a pure function that formats a single entry:

```rust
fn format_effectiveness_entry(name: &str, meta: &SkillMetadata) -> String {
    // format single skill's effectiveness data
}
```

- [ ] **Step 3: Add tests for extracted pure functions**

```rust
#[test]
fn find_dormant_skill_by_name() { /* present → Some */ }

#[test]
fn find_dormant_skill_missing() { /* absent → None */ }

#[test]
fn build_effectiveness_summary_empty_metadata() { /* no scores → default message */ }

#[test]
fn build_effectiveness_summary_with_scores() { /* scores → formatted summary */ }
```

- [ ] **Step 4: Convert test unwraps to expect with context**

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p opengoose --filter-expr 'test(sweep)'`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose/src/evolver/sweep.rs
git commit -m "refactor(evolver/sweep): extract pure functions, remove unwraps, add tests"
```

---

## Task 5: evolver/pipeline.rs — Pure function extraction + unwrap removal

**Files:**
- Modify: `crates/opengoose/src/evolver/pipeline.rs`

- [ ] **Step 1: Extract `build_existing_skill_pairs` pure function**

From `prepare_context()`, the logic that builds (name, content) pairs from loaded skills:

```rust
fn build_existing_skill_pairs(existing: &[LoadedSkill]) -> Vec<(String, String)> {
    existing.iter().map(|s| (s.name.clone(), s.content.clone())).collect()
}
```

- [ ] **Step 2: Extract `should_update_effectiveness` pure predicate**

From `update_effectiveness()`:

```rust
fn should_update_effectiveness(skill: &LoadedSkill, stamp: &stamp::Model) -> bool {
    // Check if skill matches stamp target and has metadata
    skill.metadata.is_some() && skill.name == stamp.active_skill_name()
}
```

- [ ] **Step 3: Add tests for extracted pure functions**

```rust
#[test]
fn build_existing_skill_pairs_maps_name_content() { /* 2 skills → 2 pairs */ }

#[test]
fn build_existing_skill_pairs_empty() { /* no skills → empty */ }

#[test]
fn should_update_effectiveness_matches() { /* matching skill → true */ }

#[test]
fn should_update_effectiveness_no_metadata() { /* no metadata → false */ }
```

- [ ] **Step 4: Convert test unwraps to expect with context**

- [ ] **Step 5: Run tests**

Run: `cargo nextest run -p opengoose --filter-expr 'test(pipeline)'`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose/src/evolver/pipeline.rs
git commit -m "refactor(evolver/pipeline): extract pure functions, remove unwraps, add tests"
```

---

## Task 6: proptest — CRDT merge properties

**Files:**
- Modify: `crates/opengoose-board/Cargo.toml` — add `proptest = "1"` to dev-dependencies
- Create: `crates/opengoose-board/tests/merge_props.rs`

- [ ] **Step 1: Add proptest dev-dependency**

In `crates/opengoose-board/Cargo.toml`, under `[dev-dependencies]`:

```toml
proptest = "1"
```

- [ ] **Step 2: Create Arbitrary strategies for domain types**

```rust
// tests/merge_props.rs
use opengoose_board::work_item::{Priority, Status, WorkItem, RigId};
use opengoose_board::merge::*;
use proptest::prelude::*;
use chrono::{Utc, TimeZone};

fn arb_timestamp() -> impl Strategy<Value = chrono::DateTime<Utc>> {
    // Range of timestamps within a year to generate ties
    (0i64..365 * 24 * 3600).prop_map(|secs| {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
            + chrono::Duration::seconds(secs)
    })
}

fn arb_priority() -> impl Strategy<Value = Priority> {
    prop_oneof![
        Just(Priority::P0),
        Just(Priority::P1),
        Just(Priority::P2),
    ]
}

fn arb_status() -> impl Strategy<Value = Status> {
    prop_oneof![
        Just(Status::Open),
        Just(Status::Claimed),
        Just(Status::Done),
        Just(Status::Stuck),
        Just(Status::Abandoned),
    ]
}

fn arb_tags() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-z]{1,5}", 0..5)
}

fn arb_lww_field() -> impl Strategy<Value = LwwField<String>> {
    ("[a-z]{1,10}", arb_timestamp()).prop_map(|(v, t)| LwwField { value: v, updated_at: t })
}
```

- [ ] **Step 3: Write LWW commutativity property test**

```rust
proptest! {
    #[test]
    fn lww_merge_is_commutative(a in arb_lww_field(), b in arb_lww_field()) {
        // When timestamps differ, merge is commutative
        // When timestamps are equal, self wins (tie-breaking), so a.merge(b) != b.merge(a)
        // This is expected behavior — test for timestamp-differing case
        if a.updated_at != b.updated_at {
            prop_assert_eq!(a.merge(&b).value, b.merge(&a).value);
        }
    }
}
```

- [ ] **Step 4: Write LWW idempotency property test**

```rust
proptest! {
    #[test]
    fn lww_merge_is_idempotent(a in arb_lww_field()) {
        prop_assert_eq!(a.merge(&a).value, a.value);
    }
}
```

- [ ] **Step 5: Write Priority max-register properties**

```rust
proptest! {
    #[test]
    fn priority_merge_is_commutative(a in arb_priority(), b in arb_priority()) {
        prop_assert_eq!(a.merge(&b), b.merge(&a));
    }

    #[test]
    fn priority_merge_is_associative(a in arb_priority(), b in arb_priority(), c in arb_priority()) {
        prop_assert_eq!(a.merge(&b).merge(&c), a.merge(&b.merge(&c)));
    }

    #[test]
    fn priority_merge_is_idempotent(a in arb_priority()) {
        prop_assert_eq!(a.merge(&a), a);
    }
}
```

- [ ] **Step 6: Write tags G-Set properties**

```rust
proptest! {
    #[test]
    fn tags_merge_is_commutative(a in arb_tags(), b in arb_tags()) {
        prop_assert_eq!(merge_tags(&a, &b), merge_tags(&b, &a));
    }

    #[test]
    fn tags_merge_is_idempotent(a in arb_tags()) {
        prop_assert_eq!(merge_tags(&a, &a), {
            let mut deduped: Vec<_> = a.iter().cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            deduped
        });
    }

    #[test]
    fn tags_merge_is_superset(a in arb_tags(), b in arb_tags()) {
        let merged = merge_tags(&a, &b);
        for tag in &a { prop_assert!(merged.contains(tag)); }
        for tag in &b { prop_assert!(merged.contains(tag)); }
    }
}
```

- [ ] **Step 7: Run proptest**

Run: `cargo nextest run -p opengoose-board --test merge_props`
Expected: All property tests pass (256 cases each by default).

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose-board/Cargo.toml crates/opengoose-board/tests/merge_props.rs
git commit -m "test(board): add proptest for CRDT merge properties — commutativity, associativity, idempotency"
```

---

## Task 7: proptest — State machine transitions

**Files:**
- Create: `crates/opengoose-board/tests/transition_props.rs`

- [ ] **Step 1: Write valid transition sequence property**

```rust
use opengoose_board::work_item::Status;
use proptest::prelude::*;

fn arb_status() -> impl Strategy<Value = Status> {
    prop_oneof![
        Just(Status::Open),
        Just(Status::Claimed),
        Just(Status::Done),
        Just(Status::Stuck),
        Just(Status::Abandoned),
    ]
}

proptest! {
    #[test]
    fn validate_transition_never_panics(from in arb_status(), to in arb_status()) {
        // Should always return Ok or Err, never panic
        let _ = from.validate_transition(to);
    }
}
```

- [ ] **Step 2: Write invalid transition property**

```rust
proptest! {
    #[test]
    fn done_is_terminal(to in arb_status()) {
        // Done can never transition to anything
        prop_assert!(Status::Done.validate_transition(to).is_err());
    }

    #[test]
    fn abandoned_is_terminal(to in arb_status()) {
        // Abandoned can never transition to anything
        prop_assert!(Status::Abandoned.validate_transition(to).is_err());
    }
}
```

- [ ] **Step 3: Write can_transition_to consistency property**

```rust
proptest! {
    #[test]
    fn validate_transition_matches_can_transition_to(from in arb_status(), to in arb_status()) {
        let can = from.can_transition_to(to);
        let valid = from.validate_transition(to).is_ok();
        prop_assert_eq!(can, valid);
    }
}
```

- [ ] **Step 4: Write no self-transition property**

```rust
proptest! {
    #[test]
    fn no_self_transitions(s in arb_status()) {
        // No status should transition to itself
        prop_assert!(!s.can_transition_to(s));
    }
}
```

- [ ] **Step 5: Run proptest**

Run: `cargo nextest run -p opengoose-board --test transition_props`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-board/tests/transition_props.rs
git commit -m "test(board): add proptest for state machine transition invariants"
```

---

## Task 8: proptest — Pure function robustness

**Files:**
- Modify: `crates/opengoose/src/evolver/pipeline.rs` (add proptest to existing test module)
- Modify: `crates/opengoose/Cargo.toml` (add proptest dev-dependency)

- [ ] **Step 1: Add proptest to opengoose dev-dependencies**

In `crates/opengoose/Cargo.toml`:

```toml
[dev-dependencies]
proptest = "1"
```

- [ ] **Step 2: Add validate_create_content robustness test**

In `pipeline.rs` test module:

```rust
#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn validate_create_content_never_panics(input in ".*") {
            let _ = validate_create_content(&input);
        }
    }
}
```

- [ ] **Step 3: Add build_effectiveness_summary robustness test**

In `sweep.rs` test module (if function is made pub(crate)):

```rust
proptest! {
    #[test]
    fn build_effectiveness_summary_never_panics(
        scores in prop::collection::vec(0.0f64..1.0, 0..20)
    ) {
        // Build SkillMetadata with arbitrary scores, verify no panic
        // (exact strategy depends on SkillMetadata construction)
    }
}
```

- [ ] **Step 4: Run proptest**

Run: `cargo nextest run -p opengoose --filter-expr 'test(prop_tests)'`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/Cargo.toml crates/opengoose/src/evolver/pipeline.rs crates/opengoose/src/evolver/sweep.rs
git commit -m "test(evolver): add proptest for pure function robustness"
```

---

## Task 9: Phase 3 — Remaining production unwraps

**Files:**
- Multiple files across all crates (determined dynamically)

- [ ] **Step 1: Identify remaining unwrap locations**

Run: `cargo clippy -- -W clippy::unwrap_used 2>&1 | grep "unwrap_used" | head -50`

This reveals all remaining `.unwrap()` in non-test code.

- [ ] **Step 2a: Fix opengoose-board remaining unwraps**

Focus on `work_items/helpers.rs`, `relations.rs`, `store.rs`. Apply:
- `.unwrap()` → `?` with `.context("why")` where in a Result-returning function
- `.unwrap()` → `.expect("invariant: why this is safe")` where truly infallible

- [ ] **Step 2b: Fix opengoose-rig remaining unwraps**

Focus on `middleware.rs`, `rig.rs`, `conversation_log/io.rs`.

- [ ] **Step 2c: Fix opengoose-skills remaining unwraps**

Focus on `manage/discover/mod.rs`, `loader.rs`.

- [ ] **Step 2d: Fix opengoose (binary) remaining unwraps**

Focus on `main.rs` (79 unwraps), `tui/event/keys.rs`, `web/api/board.rs`.
Update function signatures to `Result<T>` if needed, propagate to callers.

- [ ] **Step 3: Run full test suite**

Run: `cargo nextest run`
Expected: All 592+ tests pass.

- [ ] **Step 4: Commit**

```bash
git add -u
git commit -m "refactor: remove production unwraps — context-rich error propagation"
```

---

## Task 10: Phase 4 — Test code .unwrap() → .expect()

**Files:**
- All `#[cfg(test)]` modules across the codebase

- [ ] **Step 1: Convert test unwraps systematically**

Pattern: `.unwrap()` → `.expect("descriptive reason")`

Examples:
- `board.post(req).await.unwrap()` → `board.post(req).await.expect("post should succeed for valid request")`
- `tempdir().unwrap()` → `tempdir().expect("temp dir creation should succeed")`
- `fs::write(...).unwrap()` → `fs::write(...).expect("test fixture write should succeed")`
- `serde_json::from_str(...).unwrap()` → `serde_json::from_str(...).expect("test JSON should parse")`

- [ ] **Step 2: Extract common test helpers where repeated**

If the same setup pattern (tempdir + fixtures) appears in 3+ test modules, extract to a shared `testutil` module.

- [ ] **Step 3: Run full test suite**

Run: `cargo nextest run`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add -u
git commit -m "test: convert test unwraps to expect with descriptive messages"
```

---

## Task 11: Final verification + PR

- [ ] **Step 1: Run full test suite**

Run: `cargo nextest run`
Expected: All tests pass, test count increased from 592.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Verify no remaining production unwraps in high-density files**

Run: `cargo clippy -- -W clippy::unwrap_used 2>&1 | grep -c "unwrap_used"`
Expected: Significantly reduced from baseline.

- [ ] **Step 4: Commit any final fixes**

- [ ] **Step 5: Create PR**

Title: `refactor: FP quality sweep v2 — pure functions, proptest, unwrap removal`

Body should summarize:
- Number of pure functions extracted
- Number of unwraps removed
- Number of new tests added (unit + proptest)
- Proptest coverage (CRDT, state machine, parsers)
