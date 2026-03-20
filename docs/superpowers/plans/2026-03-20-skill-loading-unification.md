# Skill Loading Unification & Feedback Loop Closure

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the skill injection feedback loop — Evolver-generated skills actually reach agents, injection is tracked, and UPDATE responses produce real skill refinement.

**Architecture:** Three independent fixes to the skill pipeline. (A) Replace fragmented skill loading with `load_skills_for()`. (B) Track injection count in metadata. (C) Implement `EvolveAction::Update` to refine existing skills instead of silently dropping them.

**Tech Stack:** Rust, Goose Agent API, serde_json, chrono, tempfile (tests)

---

## File Map

| File | Changes |
|---|---|
| `crates/opengoose/src/skills/load.rs` | Rename `load_skills_3_scope` → `load_skills_for` (simpler signature), delete `load_skills()`, `load_skills_from()`, `build_catalog()`. Fix `injected_count` tracking. Move `parse_skill_header` here from middleware. |
| `crates/opengoose-rig/src/middleware.rs` | `pre_hydrate` takes `skill_catalog: &str` param. Delete `load_skill_catalog()`. Keep `parse_skill_header` as re-export or move to load.rs. |
| `crates/opengoose/src/skills/evolve.rs` | Add `build_update_prompt()`, `reset_effectiveness()`. |
| `crates/opengoose/src/evolver.rs` | Implement `Update(name)` branch. |
| `crates/opengoose/src/skills/list.rs` | Update `load_skills_3_scope` → `load_skills_for` call. |
| `crates/opengoose/src/web/api.rs` | Update `load_skills_3_scope` → `load_skills_for` call. |

### Dependency direction constraint

```
opengoose (binary) ──depends-on──→ opengoose-rig ──depends-on──→ opengoose-board
```

`opengoose-rig` cannot import from `opengoose`. So `parse_skill_header` (currently in `opengoose-rig/middleware.rs`) is called by `load.rs` (in `opengoose`). Two options:

1. Keep `parse_skill_header` in `opengoose-rig` (status quo) — `load.rs` already imports it
2. Move `parse_skill_header` to `load.rs` — cleaner, but middleware.rs also uses it internally

**Decision:** Keep `parse_skill_header` in `opengoose-rig/middleware.rs`. It's a parsing utility that load.rs imports — this cross-crate call already works and is fine.

---

## Task 1: Unify skill loading API

Rename `load_skills_3_scope` → `load_skills_for` with simplified signature. Delete unused convenience wrappers. Update all callers.

**Files:**
- Modify: `crates/opengoose/src/skills/load.rs`
- Modify: `crates/opengoose/src/evolver.rs:90`
- Modify: `crates/opengoose/src/skills/list.rs:2,22`
- Modify: `crates/opengoose/src/web/api.rs:364,371`

- [ ] **Step 1: Write test for `load_skills_for`**

The existing `load_skills_3_scope_test` test exercises the right behavior. Rename it and update the call to use the new signature.

```rust
#[test]
fn load_skills_for_loads_all_scopes() {
    let tmp = tempfile::tempdir().unwrap();

    // Global installed
    let global = tmp.path().join("global/installed/skill-a");
    std::fs::create_dir_all(&global).unwrap();
    std::fs::write(
        global.join("SKILL.md"),
        "---\nname: skill-a\ndescription: Global skill\n---\n",
    )
    .unwrap();

    // Rig learned
    let rig = tmp.path().join("rigs/worker-1/skills/learned/skill-b");
    std::fs::create_dir_all(&rig).unwrap();
    std::fs::write(
        rig.join("SKILL.md"),
        "---\nname: skill-b\ndescription: Use when testing\n---\n",
    )
    .unwrap();

    // Temporarily override home for test — use load_skills_for_with_paths
    let skills = load_skills_for_with_paths(
        &tmp.path().join("global"),
        None,
        Some("worker-1"),
        &tmp.path().join("rigs"),
    );
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0].name, "skill-b");
    assert_eq!(skills[1].name, "skill-a");
}
```

Note: Because `load_skills_for` will resolve paths from `dirs::home_dir()` internally, tests need the `_with_paths` variant (the current signature). Expose:
- `pub fn load_skills_for(rig_id, project_dir)` — resolves global/rigs from home
- `fn load_skills_for_with_paths(global, project, rig_id, rigs_base)` — testable, package-private

- [ ] **Step 2: Rename and simplify**

In `load.rs`:

```rust
/// Load all skills visible to a rig. Resolves global/rigs paths from home dir.
pub fn load_skills_for(
    rig_id: Option<&str>,
    project_dir: Option<&Path>,
) -> Vec<LoadedSkill> {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let global_dir = home.join(".opengoose/skills");
    let rigs_base = home.join(".opengoose/rigs");
    load_skills_for_with_paths(&global_dir, project_dir, rig_id, &rigs_base)
}

/// Testable version with explicit paths.
fn load_skills_for_with_paths(
    global_dir: &Path,
    project_dir: Option<&Path>,
    rig_id: Option<&str>,
    rigs_base: &Path,
) -> Vec<LoadedSkill> {
    // ... existing load_skills_3_scope body unchanged ...
}
```

Delete:
- `pub fn load_skills()` (line 159-164)
- `pub fn load_skills_from()` (line 168-174)
- `pub fn build_catalog()` (line 250-280)
- `fn extract_body()` (line 282-290)
- Test `load_skills_from_empty_dir` (line 436-440)
- Test `load_skills_from_populated_dir` (line 443-456)
- Test `build_catalog_formats_skills` (line 459-470)
- The `build_catalog` parts from test `empty_skills_returns_empty_catalog` (line 475)

- [ ] **Step 3: Update callers**

`evolver.rs:90`:
```rust
// Before:
let existing = load::load_skills_3_scope(&global_dir, None, Some(target_rig), &rigs_base);
// After:
let existing = load::load_skills_for(Some(target_rig), None);
```
Also remove the `home`, `global_dir`, `rigs_base` locals at lines 86-88 (no longer needed).

`list.rs:2,22`:
```rust
// Before:
use crate::skills::load::{determine_lifecycle, load_skills_3_scope, read_metadata, ...};
let skills = load_skills_3_scope(&global_dir, project_dir.as_deref(), None, &rigs_base);
// After:
use crate::skills::load::{determine_lifecycle, load_skills_for, read_metadata, ...};
let skills = load_skills_for(None, project_dir.as_deref());
```
Remove `home`, `global_dir`, `rigs_base` locals.

`web/api.rs:364,371`:
```rust
// Before:
load::load_skills_3_scope(&global_dir, project_dir.as_deref(), None, &rigs_base);
load::load_skills_3_scope(&global_dir, project_dir.as_deref(), Some(&rig_id), &rigs_base);
// After:
load::load_skills_for(None, project_dir.as_deref());
load::load_skills_for(Some(&rig_id), project_dir.as_deref());
```
Check if `global_dir` / `rigs_base` are used elsewhere in `collect_all_skills` or `skill_dirs()`. If `skill_dirs()` is only used to feed into `load_skills_3_scope`, simplify or remove it.

- [ ] **Step 4: Run tests**

Run: `cargo test -p opengoose -- skills`
Expected: all skill tests pass with renamed function.

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/skills/load.rs crates/opengoose/src/evolver.rs \
       crates/opengoose/src/skills/list.rs crates/opengoose/src/web/api.rs
git commit -m "refactor: unify skill loading into load_skills_for()"
```

---

## Task 2: Simplify middleware pre_hydrate

Remove `load_skill_catalog()` from middleware. Make `pre_hydrate` accept a catalog string so the caller (in `opengoose` crate) builds the catalog.

**Files:**
- Modify: `crates/opengoose-rig/src/middleware.rs`

- [ ] **Step 1: Change `pre_hydrate` signature**

```rust
/// pre_hydrate: inject context into system prompt before task starts.
pub async fn pre_hydrate(agent: &Agent, work_dir: &Path, skill_catalog: &str) {
    if let Some(agents_md) = load_agents_md(work_dir) {
        agent
            .extend_system_prompt("agents-md".to_string(), agents_md)
            .await;
    }

    if !skill_catalog.is_empty() {
        agent
            .extend_system_prompt("skill-catalog".to_string(), skill_catalog.to_string())
            .await;
    }
}
```

- [ ] **Step 2: Delete `load_skill_catalog()`**

Remove the entire `fn load_skill_catalog() -> String` (lines 46-79). This flat-scan function is replaced by `load_skills_for` + `build_catalog_capped` at the call site.

- [ ] **Step 3: Run tests**

Run: `cargo test -p opengoose-rig`
Run: `cargo check -p opengoose`
Expected: compiles. No callers of `pre_hydrate` exist yet, so no breakage.

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose-rig/src/middleware.rs
git commit -m "refactor: pre_hydrate accepts catalog string, remove flat skill scan"
```

---

## Task 3: Fix injected_count tracking

`update_last_included_at()` updates the timestamp but doesn't increment `injected_count`. Fix it.

**Files:**
- Modify: `crates/opengoose/src/skills/load.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn inclusion_tracking_increments_count() {
    use crate::skills::evolve::{Effectiveness, GeneratedFrom, SkillMetadata};

    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("tracked-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();

    let meta = SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id: 1,
            work_item_id: 1,
            dimension: "Quality".into(),
            score: 0.2,
        },
        generated_at: Utc::now().to_rfc3339(),
        evolver_work_item_id: None,
        last_included_at: None,
        effectiveness: Effectiveness {
            injected_count: 0,
            subsequent_scores: vec![],
        },
    };
    std::fs::write(
        skill_dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .unwrap();

    // Call twice
    update_inclusion_tracking(&skill_dir);
    update_inclusion_tracking(&skill_dir);

    let updated: SkillMetadata = serde_json::from_str(
        &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(updated.effectiveness.injected_count, 2);
    assert!(updated.last_included_at.is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opengoose -- inclusion_tracking_increments_count`
Expected: FAIL — function `update_inclusion_tracking` not found (still named `update_last_included_at`, count not incremented)

- [ ] **Step 3: Rename and fix**

```rust
fn update_inclusion_tracking(skill_path: &Path) {
    let meta_path = skill_path.join("metadata.json");
    if let Ok(content) = std::fs::read_to_string(&meta_path) {
        if let Ok(mut meta) = serde_json::from_str::<SkillMetadata>(&content) {
            meta.last_included_at = Some(Utc::now().to_rfc3339());
            meta.effectiveness.injected_count += 1;
            if let Ok(json) = serde_json::to_string_pretty(&meta) {
                let _ = std::fs::write(&meta_path, json);
            }
        }
    }
}
```

Update the call in `build_catalog_capped` (line 217):
```rust
// Before:
update_last_included_at(&skill.path);
// After:
update_inclusion_tracking(&skill.path);
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p opengoose -- skills`
Expected: all pass, including new test

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/skills/load.rs
git commit -m "fix: track injected_count when skills are included in catalog"
```

---

## Task 4: Add update prompt builder and effectiveness reset

Add the evolve.rs functions needed by the `Update` branch in Task 5.

**Files:**
- Modify: `crates/opengoose/src/skills/evolve.rs`

- [ ] **Step 1: Write test for `build_update_prompt`**

```rust
#[test]
fn build_update_prompt_includes_existing_content() {
    let existing = "---\nname: fix-auth\ndescription: Use when auth fails\n---\n# Steps\n1. Check token\n";
    let prompt = build_update_prompt(
        existing,
        "Quality",
        0.2,
        Some("missed edge case"),
        "Fix login flow",
        "user tried X, got error Y",
    );
    assert!(prompt.contains("fix-auth"));
    assert!(prompt.contains("Check token"));
    assert!(prompt.contains("missed edge case"));
    assert!(prompt.contains("Fix login flow"));
}
```

- [ ] **Step 2: Write test for `reset_effectiveness`**

```rust
#[test]
fn reset_effectiveness_clears_scores() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("test-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();

    let meta = SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id: 1,
            work_item_id: 1,
            dimension: "Quality".into(),
            score: 0.2,
        },
        generated_at: Utc::now().to_rfc3339(),
        evolver_work_item_id: None,
        last_included_at: Some(Utc::now().to_rfc3339()),
        effectiveness: Effectiveness {
            injected_count: 5,
            subsequent_scores: vec![0.3, 0.4, 0.5],
        },
    };
    std::fs::write(
        skill_dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .unwrap();

    reset_effectiveness(&skill_dir).unwrap();

    let updated: SkillMetadata = serde_json::from_str(
        &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
    )
    .unwrap();
    assert!(updated.effectiveness.subsequent_scores.is_empty());
    assert_eq!(updated.effectiveness.injected_count, 0);
    // generated_from preserved
    assert_eq!(updated.generated_from.stamp_id, 1);
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p opengoose -- build_update_prompt reset_effectiveness`
Expected: FAIL — functions not found

- [ ] **Step 4: Implement**

```rust
pub fn build_update_prompt(
    existing_content: &str,
    dimension: &str,
    score: f32,
    comment: Option<&str>,
    work_item_title: &str,
    log_summary: &str,
) -> String {
    let mut prompt = format!(
        "Update this existing skill based on a new failure.\n\n\
         ## Existing Skill\n{existing_content}\n\n\
         ## New Failure\n\
         dimension: {dimension}, score: {score:.1}, comment: '{}'\n\
         task: '{work_item_title}'\n\n",
        comment.unwrap_or("(none)"),
    );

    if !log_summary.is_empty() {
        prompt.push_str(&format!("## Conversation Log\n{log_summary}\n\n"));
    }

    prompt.push_str(
        "Merge the new lesson into the existing skill.\n\
         Output the complete updated SKILL.md with YAML frontmatter.",
    );

    prompt
}

pub fn reset_effectiveness(skill_dir: &Path) -> anyhow::Result<()> {
    let meta_path = skill_dir.join("metadata.json");
    let content = std::fs::read_to_string(&meta_path)?;
    let mut meta: SkillMetadata = serde_json::from_str(&content)?;
    meta.effectiveness.subsequent_scores.clear();
    meta.effectiveness.injected_count = 0;
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    Ok(())
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p opengoose -- build_update_prompt reset_effectiveness`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose/src/skills/evolve.rs
git commit -m "feat: add build_update_prompt and reset_effectiveness for skill refinement"
```

---

## Task 5: Implement EvolveAction::Update in evolver

Replace the TODO with actual skill update logic.

**Files:**
- Modify: `crates/opengoose/src/evolver.rs:156-161`

- [ ] **Step 1: Implement Update branch**

Replace evolver.rs lines 156-161:

```rust
evolve::EvolveAction::Update(name) => {
    // Find existing skill by name
    let target_skill = existing.iter().find(|s| s.name == name);
    match target_skill {
        Some(skill) => {
            let update_prompt = evolve::build_update_prompt(
                &skill.content,
                &stamp.dimension,
                stamp.score,
                stamp.comment.as_deref(),
                &work_item.title,
                &log_summary,
            );
            let updated_response =
                call_agent(agent, &update_prompt, evolver_item.id).await?;
            let updated_action = evolve::parse_evolve_response(&updated_response);
            match updated_action {
                evolve::EvolveAction::Create(content) => {
                    match evolve::validate_skill_output(&content) {
                        Ok(()) => {
                            std::fs::write(skill.path.join("SKILL.md"), &content)?;
                            evolve::reset_effectiveness(&skill.path)?;
                            info!(
                                "evolver: updated skill '{name}' for stamp {}",
                                stamp.id
                            );
                        }
                        Err(e) => {
                            warn!(
                                "evolver: updated skill '{name}' failed validation: {e}"
                            );
                        }
                    }
                }
                _ => {
                    warn!(
                        "evolver: UPDATE:{name} response was not a valid skill, skipping"
                    );
                }
            }
        }
        None => {
            warn!(
                "evolver: UPDATE:{name} but skill not found, skipping stamp {}",
                stamp.id
            );
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p opengoose`
Expected: compiles without errors

- [ ] **Step 3: Run all tests**

Run: `cargo test -p opengoose -- skills`
Run: `cargo test -p opengoose -- evolver`
Expected: all pass

- [ ] **Step 4: Commit**

```bash
git add crates/opengoose/src/evolver.rs
git commit -m "feat: implement EvolveAction::Update — refine existing skills instead of dropping"
```

---

## Verification

After all tasks:

- [ ] `cargo check` — full workspace compiles
- [ ] `cargo test -p opengoose` — all tests pass
- [ ] `cargo test -p opengoose-rig` — middleware tests pass
- [ ] `cargo test -p opengoose-board` — board tests unaffected
- [ ] Grep for stale references: `grep -r "load_skills_3_scope\|load_skills_from\|load_skills()" crates/` — should return zero hits outside of tests
