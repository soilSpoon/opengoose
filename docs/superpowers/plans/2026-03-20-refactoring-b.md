# Plan B Refactoring — 4-Crate Restructure

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure OpenGoose from 3 crates to 4 by extracting `opengoose-skills`, decomposing the Board God Object, eliminating duplicate code, and reducing coupling.

**Architecture:** New `opengoose-skills` crate owns all skill logic (loading, evolution, metadata, management). Board splits `impl` blocks across files. Binary crate becomes pure orchestration. All public skills functions take `base_dir: &Path` instead of calling `home_dir()`.

**Tech Stack:** Rust 2024, SeaORM, Goose, Tokio, ratatui, axum

**Spec:** `docs/superpowers/specs/2026-03-20-refactoring-b-design.md`

---

## Task 1: Create opengoose-skills crate scaffold

**Files:**
- Create: `crates/opengoose-skills/Cargo.toml`
- Create: `crates/opengoose-skills/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create crate directory and Cargo.toml**

```toml
# crates/opengoose-skills/Cargo.toml
[package]
name = "opengoose-skills"
description = "Skill loading, evolution, metadata, and management for OpenGoose"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
chrono = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create empty lib.rs**

```rust
// crates/opengoose-skills/src/lib.rs
// opengoose-skills — Skill loading, evolution, metadata, management
//
// No dependency on board, rig, or goose.
// All public functions take base_dir: &Path for filesystem root.
```

- [ ] **Step 3: Add to workspace**

In root `Cargo.toml`, add `"crates/opengoose-skills"` to `[workspace.members]` and add:
```toml
opengoose-skills = { path = "crates/opengoose-skills" }
```

- [ ] **Step 4: Add dependency to opengoose-rig and opengoose**

In `crates/opengoose-rig/Cargo.toml` add:
```toml
opengoose-skills = { workspace = true }
```

In `crates/opengoose/Cargo.toml` add:
```toml
opengoose-skills = { workspace = true }
```

- [ ] **Step 5: Verify**

Run: `cargo check --workspace`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-skills/ Cargo.toml Cargo.lock crates/opengoose/Cargo.toml crates/opengoose-rig/Cargo.toml
git commit -m "feat: create opengoose-skills crate scaffold"
```

---

## Task 2: Move metadata types and frontmatter parsing to opengoose-skills

**Files:**
- Create: `crates/opengoose-skills/src/metadata.rs`
- Modify: `crates/opengoose-skills/src/lib.rs`
- Modify: `crates/opengoose/src/skills/evolve.rs` — remove moved types, import from skills crate
- Modify: `crates/opengoose/src/skills/discover.rs` — use shared parse_frontmatter
- Modify: `crates/opengoose-rig/src/middleware.rs` — use shared parse_frontmatter

These types are currently in `crates/opengoose/src/skills/evolve.rs` lines 226-253 and `discover.rs`.

- [ ] **Step 1: Write test for parse_frontmatter**

```rust
// crates/opengoose-skills/src/metadata.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_valid() {
        let content = "---\nname: test-skill\ndescription: Use when testing\n---\n# Body";
        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.description, "Use when testing");
    }

    #[test]
    fn parse_frontmatter_missing_frontmatter() {
        assert!(parse_frontmatter("# No frontmatter").is_none());
    }

    #[test]
    fn parse_frontmatter_missing_name() {
        let content = "---\ndescription: Use when testing\n---\n";
        assert!(parse_frontmatter(content).is_none());
    }

    #[test]
    fn parse_frontmatter_missing_description() {
        let content = "---\nname: test\n---\n";
        assert!(parse_frontmatter(content).is_none());
    }

    #[test]
    fn metadata_roundtrip() {
        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 5,
                work_item_id: 42,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: "2026-03-19T10:00:00Z".into(),
            evolver_work_item_id: Some(100),
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 1,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: SkillMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.generated_from.stamp_id, 5);
    }

    #[test]
    fn read_metadata_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1, work_item_id: 1,
                dimension: "Quality".into(), score: 0.2,
            },
            generated_at: "2026-03-20T00:00:00Z".into(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness { injected_count: 0, subsequent_scores: vec![] },
            skill_version: 1,
        };
        write_metadata(&skill_dir, &meta).unwrap();
        let loaded = read_metadata(&skill_dir).unwrap();
        assert_eq!(loaded.generated_from.stamp_id, 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opengoose-skills`
Expected: FAIL — types not defined yet

- [ ] **Step 3: Implement metadata.rs**

Move types from `evolve.rs:226-253` and add `parse_frontmatter()`, `read_metadata()`, `write_metadata()`:

```rust
// crates/opengoose-skills/src/metadata.rs
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
}

pub fn parse_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];

    let mut name = None;
    let mut description = None;
    for line in frontmatter.lines() {
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().trim_matches('"').to_string());
        }
        if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().trim_matches('"').to_string());
        }
    }

    Some(SkillFrontmatter { name: name?, description: description? })
}

fn default_version() -> u32 { 1 }

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub generated_from: GeneratedFrom,
    pub generated_at: String,
    pub evolver_work_item_id: Option<i64>,
    pub last_included_at: Option<String>,
    pub effectiveness: Effectiveness,
    #[serde(default = "default_version")]
    pub skill_version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneratedFrom {
    pub stamp_id: i64,
    pub work_item_id: i64,
    pub dimension: String,
    pub score: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Effectiveness {
    pub injected_count: u32,
    pub subsequent_scores: Vec<f32>,
}

pub fn read_metadata(skill_dir: &Path) -> Option<SkillMetadata> {
    let meta_path = skill_dir.join("metadata.json");
    let content = std::fs::read_to_string(meta_path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn write_metadata(skill_dir: &Path, meta: &SkillMetadata) -> anyhow::Result<()> {
    let meta_path = skill_dir.join("metadata.json");
    std::fs::write(meta_path, serde_json::to_string_pretty(meta)?)?;
    Ok(())
}
```

- [ ] **Step 4: Add module to lib.rs**

```rust
pub mod metadata;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p opengoose-skills`
Expected: all tests PASS

- [ ] **Step 6: Update evolve.rs to re-export from opengoose-skills**

In `crates/opengoose/src/skills/evolve.rs`, replace the type definitions (lines 226-253) with:
```rust
pub use opengoose_skills::metadata::{SkillMetadata, GeneratedFrom, Effectiveness};
```
Remove `default_version()`. Keep all functions that use these types.

- [ ] **Step 7: Update middleware.rs to use shared parse_frontmatter**

In `crates/opengoose-rig/src/middleware.rs`, replace `parse_skill_header()` body:
```rust
pub fn parse_skill_header(content: &str) -> Option<(String, String)> {
    let fm = opengoose_skills::metadata::parse_frontmatter(content)?;
    Some((fm.name, fm.description))
}
```

- [ ] **Step 8: Verify full workspace**

Run: `cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 9: Commit**

```bash
git add crates/opengoose-skills/src/metadata.rs crates/opengoose-skills/src/lib.rs \
  crates/opengoose/src/skills/evolve.rs crates/opengoose-rig/src/middleware.rs
git commit -m "refactor: move metadata types + parse_frontmatter to opengoose-skills"
```

---

## Task 3: Move evolution modules to opengoose-skills

**Files:**
- Create: `crates/opengoose-skills/src/evolution/mod.rs`
- Create: `crates/opengoose-skills/src/evolution/parser.rs`
- Create: `crates/opengoose-skills/src/evolution/validator.rs`
- Create: `crates/opengoose-skills/src/evolution/prompts.rs`
- Create: `crates/opengoose-skills/src/evolution/writer.rs`
- Modify: `crates/opengoose-skills/src/lib.rs`
- Modify: `crates/opengoose/src/skills/evolve.rs` — becomes thin re-export
- Modify: `crates/opengoose/src/evolver.rs` — update imports

Source: `crates/opengoose/src/skills/evolve.rs` (942 lines)

- [ ] **Step 1: Create evolution/parser.rs with tests**

Move `EvolveAction`, `parse_evolve_response()`, `SweepDecision`, `parse_sweep_response()` from evolve.rs lines 18-83. Include all existing tests from evolve.rs that test these functions.

```rust
// crates/opengoose-skills/src/evolution/parser.rs

#[derive(Debug, PartialEq)]
pub enum EvolveAction {
    Create(String),
    Update(String),
    Skip,
}

pub fn parse_evolve_response(response: &str) -> EvolveAction {
    let trimmed = response.trim();
    if trimmed == "SKIP" {
        return EvolveAction::Skip;
    }
    if let Some(name) = trimmed.strip_prefix("UPDATE:") {
        return EvolveAction::Update(name.trim().to_string());
    }
    EvolveAction::Create(trimmed.to_string())
}

#[derive(Debug, PartialEq)]
pub enum SweepDecision {
    Restore(String),
    Refine(String, String),
    Keep(String),
    Delete(String),
}

pub fn parse_sweep_response(response: &str) -> Vec<SweepDecision> {
    // ... move existing implementation from evolve.rs lines 48-83
}
```

Include all 7 parser tests from evolve.rs (3 evolve + 4 sweep).

- [ ] **Step 2: Create evolution/validator.rs with tests**

Move `validate_skill_output()` from evolve.rs lines 89-128. Replace inline frontmatter parsing with `crate::metadata::parse_frontmatter()`.

```rust
// crates/opengoose-skills/src/evolution/validator.rs
use crate::metadata::parse_frontmatter;

pub fn validate_skill_output(content: &str) -> anyhow::Result<()> {
    let content = content.trim();
    let fm = parse_frontmatter(content)
        .ok_or_else(|| anyhow::anyhow!("missing or invalid YAML frontmatter"))?;

    if fm.name.is_empty() || fm.name.len() > 64 {
        anyhow::bail!("name must be 1-64 chars, got {}", fm.name.len());
    }
    if !fm.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        anyhow::bail!("name must be lowercase + hyphens only: {}", fm.name);
    }
    if !fm.description.starts_with("Use when") {
        anyhow::bail!("description must start with 'Use when', got: {}", fm.description);
    }
    Ok(())
}
```

Include 4 validation tests from evolve.rs.

- [ ] **Step 3: Create evolution/prompts.rs with tests**

Move `build_evolve_prompt()`, `build_update_prompt()`, `build_sweep_prompt()`, `summarize_for_prompt()` from evolve.rs. Remove `read_conversation_log()` — the caller will pass the log string.

Include all 5 prompt tests from evolve.rs (2 evolve + 1 update + 2 sweep).

Note: `read_conversation_log()` (evolve.rs line 222) calls `opengoose_rig::conversation_log::read_log()` — this function does NOT move to opengoose-skills (would create circular dependency). Instead, move the call to `evolver.rs` which already has access to opengoose-rig. The prompts module only receives the log string as a parameter.

- [ ] **Step 4: Create evolution/writer.rs with tests**

Move `write_skill_to_rig_scope()`, `update_existing_skill()`, `refine_skill()`, `update_effectiveness_versioned()`, `build_active_versions_json()` from evolve.rs.

Key change: `write_skill_to_rig_scope()` takes `base_dir: &Path` instead of calling `crate::home_dir()`:

```rust
pub fn write_skill_to_rig_scope(
    base_dir: &Path,  // NEW — was crate::home_dir()
    rig_id: &str,
    skill_content: &str,
    stamp_id: i64,
    work_item_id: i64,
    dimension: &str,
    score: f32,
    evolver_work_item_id: Option<i64>,
) -> anyhow::Result<String> {
    let name = extract_name_from_content(skill_content)
        .ok_or_else(|| anyhow::anyhow!("cannot extract name from skill content"))?;
    let skill_dir = base_dir.join(format!(".opengoose/rigs/{rig_id}/skills/learned/{name}"));
    // ... rest unchanged
}
```

Include 6 writer/effectiveness tests from evolve.rs. Tests use `tempfile::tempdir()` as base_dir.

- [ ] **Step 5: Create evolution/mod.rs**

```rust
pub mod parser;
pub mod prompts;
pub mod validator;
pub mod writer;
```

- [ ] **Step 6: Update lib.rs**

```rust
pub mod evolution;
pub mod metadata;
```

- [ ] **Step 7: Run opengoose-skills tests**

Run: `cargo test -p opengoose-skills`
Expected: all tests PASS

- [ ] **Step 8: Slim down evolve.rs in binary crate**

Replace `crates/opengoose/src/skills/evolve.rs` with re-exports only:

```rust
// Thin re-export layer — all logic lives in opengoose-skills
pub use opengoose_skills::evolution::parser::{EvolveAction, SweepDecision, parse_evolve_response, parse_sweep_response};
pub use opengoose_skills::evolution::validator::validate_skill_output;
pub use opengoose_skills::evolution::prompts::*;
pub use opengoose_skills::evolution::writer::*;
pub use opengoose_skills::metadata::{SkillMetadata, GeneratedFrom, Effectiveness};
```

- [ ] **Step 9: Update evolver.rs imports**

In `crates/opengoose/src/evolver.rs`, change `use crate::skills::{evolve, load}` to use `opengoose_skills` directly where needed. Update `write_skill_to_rig_scope` calls to pass `crate::home_dir()` as first arg.

- [ ] **Step 10: Verify**

Run: `cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 11: Commit**

```bash
git add crates/opengoose-skills/src/evolution/ crates/opengoose/src/skills/evolve.rs crates/opengoose/src/evolver.rs
git commit -m "refactor: move evolution modules to opengoose-skills"
```

---

## Task 4: Move loader, lifecycle, catalog to opengoose-skills

**Files:**
- Create: `crates/opengoose-skills/src/loader.rs`
- Create: `crates/opengoose-skills/src/lifecycle.rs`
- Create: `crates/opengoose-skills/src/catalog.rs`
- Modify: `crates/opengoose-skills/src/lib.rs`
- Modify: `crates/opengoose/src/skills/load.rs` — becomes thin re-export

Source: `crates/opengoose/src/skills/load.rs` (824 lines)

- [ ] **Step 1: Create lifecycle.rs**

Move `Lifecycle` enum and `determine_lifecycle()` from load.rs lines 14-46.

```rust
// crates/opengoose-skills/src/lifecycle.rs
use chrono::Utc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Active,
    Dormant,
    Archived,
}

pub fn determine_lifecycle(generated_at: &str, last_included_at: Option<&str>) -> Lifecycle {
    // ... move existing implementation from load.rs
}
```

Include test.

- [ ] **Step 2: Create loader.rs**

Move `LoadedSkill`, `SkillScope`, `load_skills_for_with_paths()`, `scan_scope()`, `load_dormant_and_archived()`, `update_inclusion_tracking()`, `extract_body()` from load.rs.

Key change: `load_skills_for()` becomes `load_skills(base_dir, rig_id, project_dir)`:

```rust
// crates/opengoose-skills/src/loader.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillScope {
    Installed, // manually installed, no decay
    Learned,   // auto-generated, lifecycle managed
}

#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub name: String,
    pub description: String,
    pub path: std::path::PathBuf,
    pub content: String,
    pub scope: SkillScope,
}

/// Load skills for a rig, with explicit base paths.
pub fn load_skills(
    base_dir: &Path,       // was home_dir()
    rig_id: Option<&str>,
    project_dir: Option<&Path>,
) -> Vec<LoadedSkill> {
    let global_dir = base_dir.join(".opengoose/skills");
    let rigs_base = base_dir.join(".opengoose/rigs");
    load_skills_with_paths(&global_dir, project_dir, rig_id, &rigs_base)
}
```

Move `read_metadata` call sites to use `crate::metadata::read_metadata`.
Move `is_effective` to `metadata.rs`.
Move `extract_body()` (load.rs line 241) to `loader.rs` (parses skill content, removing frontmatter).
Move `update_inclusion_tracking()` (load.rs line 214) to `lifecycle.rs` (updates `last_included_at` in metadata).

Note: `load.rs:73` uses `dirs::home_dir()` directly (not `crate::home_dir()`). After extraction, the `load_skills()` function takes `base_dir: &Path` so neither `dirs` nor `crate::home_dir()` is needed.

Include existing tests, adapting `home_dir()` / `dirs::home_dir()` calls to use `tempdir()`.

- [ ] **Step 3: Create catalog.rs**

`build_catalog_capped()` is currently test-only (defined inside `#[cfg(test)]` in load.rs line 256). Promote it to a public function since it is the catalog generation logic that middleware uses to inject skills into system prompts.

```rust
// crates/opengoose-skills/src/catalog.rs
use crate::loader::{LoadedSkill, SkillScope};
use crate::lifecycle::Lifecycle;
use crate::metadata;

/// Build catalog string for system prompt injection.
/// Installed skills first, then effective learned, then unknown, capped at `cap`.
/// Excludes dormant/archived and ineffective learned skills.
pub fn build_catalog(skills: &[LoadedSkill], cap: usize) -> String {
    // ... promote existing test-only implementation to public
}
```

- [ ] **Step 4: Add is_effective to metadata.rs**

Move from load.rs line 231:
```rust
// in metadata.rs
pub fn is_effective(meta: &SkillMetadata) -> Option<bool> {
    let scores = &meta.effectiveness.subsequent_scores;
    if scores.len() < 3 { return None; }
    let avg = scores.iter().sum::<f32>() / scores.len() as f32;
    let improvement = avg - meta.generated_from.score;
    Some(improvement >= 0.2)
}
```

- [ ] **Step 5: Update lib.rs**

```rust
pub mod catalog;
pub mod evolution;
pub mod lifecycle;
pub mod loader;
pub mod metadata;
```

- [ ] **Step 6: Verify opengoose-skills**

Run: `cargo test -p opengoose-skills`
Expected: PASS

- [ ] **Step 7: Slim down load.rs in binary crate**

Replace with re-exports:
```rust
pub use opengoose_skills::loader::*;
pub use opengoose_skills::lifecycle::*;
pub use opengoose_skills::catalog::*;
pub use opengoose_skills::metadata::read_metadata;
```

Keep `load_skills_for()` as a thin wrapper that calls `opengoose_skills::loader::load_skills(crate::home_dir(), ...)` for backward compat within the binary crate.

- [ ] **Step 8: Verify workspace**

Run: `cargo test --workspace`
Expected: all PASS

- [ ] **Step 9: Commit**

```bash
git add crates/opengoose-skills/src/{loader,lifecycle,catalog}.rs crates/opengoose-skills/src/lib.rs \
  crates/opengoose-skills/src/metadata.rs crates/opengoose/src/skills/load.rs
git commit -m "refactor: move loader, lifecycle, catalog to opengoose-skills"
```

---

## Task 5: Move manage modules to opengoose-skills

**Files:**
- Create: `crates/opengoose-skills/src/manage/mod.rs`
- Create: `crates/opengoose-skills/src/manage/{add,remove,update,promote,discover,list,lock}.rs`
- Create: `crates/opengoose-skills/src/source.rs`
- Create: `crates/opengoose-skills/src/test_utils.rs`
- Modify: `crates/opengoose-skills/src/lib.rs`
- Modify: `crates/opengoose/src/skills/mod.rs` — dispatch calls opengoose_skills
- Delete (contents only): `crates/opengoose/src/skills/{add,remove,update,promote,discover,list,lock,source}.rs`

- [ ] **Step 1: Create test_utils.rs with IsolatedEnv**

```rust
// crates/opengoose-skills/src/test_utils.rs
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub struct IsolatedEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
    prev_home: Option<String>,
    prev_xdg: Option<String>,
}

#[cfg(test)]
impl IsolatedEnv {
    pub fn new(tmp: &Path) -> Self {
        let guard = ENV_LOCK.lock().unwrap();
        let prev_home = std::env::var("HOME").ok();
        let prev_xdg = std::env::var("XDG_STATE_HOME").ok();
        unsafe {
            std::env::set_var("HOME", tmp);
            std::env::set_var("XDG_STATE_HOME", tmp.join("xdg"));
        }
        Self { _guard: guard, prev_home, prev_xdg }
    }
}

#[cfg(test)]
impl Drop for IsolatedEnv {
    fn drop(&mut self) {
        match &self.prev_home {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        match &self.prev_xdg {
            Some(v) => unsafe { std::env::set_var("XDG_STATE_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_STATE_HOME") },
        }
    }
}

#[cfg(test)]
pub fn skill_path(base: &Path, scope: &str, name: &str) -> std::path::PathBuf {
    base.join(format!(".opengoose/skills/{scope}/{name}"))
}
```

- [ ] **Step 2: Move source.rs (no dependencies)**

Copy `crates/opengoose/src/skills/source.rs` → `crates/opengoose-skills/src/source.rs` unchanged.

- [ ] **Step 3: Move manage modules one by one**

For each of `add.rs`, `remove.rs`, `update.rs`, `promote.rs`, `discover.rs`, `list.rs`, `lock.rs`:
1. Copy to `crates/opengoose-skills/src/manage/`
2. Replace `crate::home_dir()` with `base_dir: &Path` parameter
3. Replace `crate::ENV_LOCK` with `crate::test_utils::IsolatedEnv` in tests
4. Replace `use crate::skills::*` with `use crate::*`

Example for `add.rs`:
```rust
// Before: pub async fn run(source: &str, all: bool, skill: Option<&str>, global: bool) -> Result<()>
// After:  pub async fn run(base_dir: &Path, source: &str, all: bool, skill: Option<&str>, global: bool) -> Result<()>
```

- [ ] **Step 4: Create manage/mod.rs**

```rust
pub mod add;
pub mod discover;
pub mod list;
pub mod lock;
pub mod promote;
pub mod remove;
pub mod update;
```

- [ ] **Step 5: Update lib.rs**

```rust
pub mod catalog;
pub mod evolution;
pub mod lifecycle;
pub mod loader;
pub mod manage;
pub mod metadata;
pub mod source;
#[cfg(test)]
pub(crate) mod test_utils;
```

- [ ] **Step 6: Verify opengoose-skills**

Run: `cargo test -p opengoose-skills`
Expected: PASS

- [ ] **Step 7: Update binary crate skills/mod.rs dispatch**

```rust
// crates/opengoose/src/skills/mod.rs
use clap::Subcommand;

// Re-export for internal use
pub use opengoose_skills::loader as load;
pub use opengoose_skills::evolution::parser as evolve_parser;

#[derive(Subcommand)]
pub enum SkillsAction {
    // ... same as before
}

pub async fn run_skills_command(action: SkillsAction) -> anyhow::Result<()> {
    let base_dir = crate::home_dir();
    match action {
        SkillsAction::Add { source, all, skill, global } =>
            opengoose_skills::manage::add::run(&base_dir, &source, all, skill.as_deref(), global).await,
        SkillsAction::List { global, archived } =>
            opengoose_skills::manage::list::run(&base_dir, global, archived),
        SkillsAction::Remove { name, global } =>
            opengoose_skills::manage::remove::run(&base_dir, &name, global),
        SkillsAction::Update =>
            opengoose_skills::manage::update::run(&base_dir).await,
        SkillsAction::Promote { name, to, from_rig, force } =>
            opengoose_skills::manage::promote::run(&base_dir, &name, &to, from_rig.as_deref(), force),
    }
}
```

- [ ] **Step 8: Remove old skill files from binary crate**

Delete content of: `crates/opengoose/src/skills/{add,remove,update,promote,discover,list,lock,source,load,evolve}.rs`
Each becomes either empty or a single `pub use opengoose_skills::...` if needed elsewhere.

- [ ] **Step 9: Verify workspace**

Run: `cargo test --workspace`
Expected: all PASS

- [ ] **Step 10: Commit**

```bash
git add crates/opengoose-skills/src/{manage/,source.rs,test_utils.rs,lib.rs} \
  crates/opengoose/src/skills/
git commit -m "refactor: move manage modules + test_utils to opengoose-skills"
```

---

## Task 6: Split Board impl across files

**Files:**
- Create: `crates/opengoose-board/src/work_items.rs`
- Create: `crates/opengoose-board/src/rigs.rs`
- Create: `crates/opengoose-board/src/stamp_ops.rs`
- Modify: `crates/opengoose-board/src/board.rs` — keep struct + init only
- Modify: `crates/opengoose-board/src/lib.rs` — add new modules

No logic changes — just moving `impl Board` blocks to separate files.

- [ ] **Step 1: Create work_items.rs**

Move these methods from board.rs to a new `impl Board` block:
- `post` (line 86)
- `claim` (line 115)
- `submit` (line 139)
- `unclaim` (line 152)
- `mark_stuck` (line 168)
- `retry` (line 190)
- `abandon` (line 205)
- `get` (line 215)
- `list` (line 223)
- `ready` (line 231) — includes private `blocked_item_ids()`
- `claimed_by` (line 249)
- `completed_by_rig` (line 265)

Also move private helpers: `get_or_err()`, `find_model()`, `to_work_item()`, `blocked_item_ids()`.

```rust
// crates/opengoose-board/src/work_items.rs
use crate::board::Board;
use crate::entity;
use crate::work_item::*;
use sea_orm::*;
// ...

impl Board {
    pub async fn post(&self, req: PostWorkItem) -> Result<WorkItem, BoardError> {
        // ... exact existing code
    }
    // ... all work item methods
}
```

- [ ] **Step 2: Create rigs.rs**

Move: `register_rig`, `list_rigs`, `get_rig`, `remove_rig`.

```rust
// crates/opengoose-board/src/rigs.rs
use crate::board::Board;
use crate::entity;
use crate::work_item::BoardError;
use sea_orm::*;

impl Board {
    pub async fn register_rig(&self, id: &str, name: &str, model: &str) -> Result<(), BoardError> {
        // ... exact existing code
    }
    // ...
}
```

- [ ] **Step 3: Create stamp_ops.rs**

Move: `add_stamp`, `stamps_for_item`, `weighted_score`, `trust_level`, `stamps_with_scores`, `batch_rig_scores`, `unprocessed_low_stamps`, `recent_low_stamps`, `mark_stamp_evolved`.

Also add new method `stamps_for_rig()`:

```rust
pub async fn stamps_for_rig(&self, rig_id: &str) -> Result<Vec<entity::stamp::Model>, BoardError> {
    entity::stamp::Entity::find()
        .filter(entity::stamp::Column::TargetRig.eq(rig_id))
        .all(&self.db)
        .await
        .map_err(|e| BoardError::Database(e.to_string()))
}
```

- [ ] **Step 4: Slim down board.rs**

Keep only: `Board` struct definition, `connect()`, `in_memory()`, `create_tables()`, `ensure_columns()`, `ensure_system_rigs()`, `wait_for_claimable()`, `notify_handle()`, `stamp_notify_handle()`, `add_dependency()`, `would_create_cycle()`.

Make `db` field `pub(crate)` (not `pub`) so sibling files can access it. Remove `pub fn db()`.

- [ ] **Step 5: Update lib.rs**

```rust
pub mod beads;
pub mod board;
pub mod entity;
pub mod relations;
pub mod rigs;
pub mod stamp_ops;
pub mod stamps;
pub mod work_item;
pub mod work_items;

pub use board::{AddStampParams, Board};
pub use stamps::TrustLevel;
pub use work_item::{BoardError, PostWorkItem, Priority, RigId, Status, WorkItem};
```

- [ ] **Step 6: Verify**

Run: `cargo test -p opengoose-board`
Expected: all PASS. Board tests may need to move to the file where their tested methods now live.

- [ ] **Step 7: Verify workspace**

Run: `cargo test --workspace`
Expected: all PASS

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose-board/src/
git commit -m "refactor: split Board impl across work_items, rigs, stamp_ops"
```

---

## Task 7: Remove Board.db() public accessor

**Files:**
- Modify: `crates/opengoose-board/src/board.rs` — remove `pub fn db()`

Note: `Board.db()` has no external callers in the current code (the api.rs coupling was already fixed via `stamps_with_scores()` and `batch_rig_scores()`). This task simply removes the public accessor to prevent future coupling.

- [ ] **Step 1: Remove Board.db() from board.rs**

Delete `pub fn db(&self) -> &DatabaseConnection` (line 541). The `db` field is already `pub(crate)` after Task 6, so internal Board impl files still have access.

- [ ] **Step 2: Verify no external callers**

Run: `cargo check --workspace`
Expected: compiles cleanly (no external callers exist)

- [ ] **Step 3: Commit**

```bash
git add crates/opengoose-board/src/board.rs
git commit -m "refactor: remove Board.db() public accessor"
```

---

## Task 8: Split main.rs into cli, runtime, commands

**Files:**
- Create: `crates/opengoose/src/cli.rs`
- Create: `crates/opengoose/src/runtime.rs`
- Create: `crates/opengoose/src/commands/mod.rs`
- Create: `crates/opengoose/src/commands/board.rs`
- Create: `crates/opengoose/src/commands/rigs.rs`
- Modify: `crates/opengoose/src/main.rs` — slim down to ~100 lines

- [ ] **Step 1: Extract cli.rs**

Move `Cli`, `Commands`, `BoardAction`, `RigsAction` structs (main.rs lines 46-140) to `cli.rs`:

```rust
// crates/opengoose/src/cli.rs
use clap::{Parser, Subcommand};
use crate::skills::SkillsAction;
use crate::logs::LogsAction;

#[derive(Parser)]
#[command(name = "opengoose", version = "0.2.0")]
#[command(about = "Goose-native pull architecture with Wasteland-level agent autonomy")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[arg(long, default_value = "1355", global = true)]
    pub port: u16,
}

#[derive(Subcommand)]
pub enum Commands {
    // ... move all variants
}

#[derive(Subcommand)]
pub enum BoardAction {
    // ... move all variants
}

#[derive(Subcommand)]
pub enum RigsAction {
    // ... move all variants
}
```

- [ ] **Step 2: Extract runtime.rs with unified Agent creation**

Move `create_base_agent()`, `create_operator_agent()`, `create_worker_agent()` from main.rs.
Also absorb `create_evolver_agent()` from evolver.rs.

```rust
// crates/opengoose/src/runtime.rs
use anyhow::{Context, Result};
use goose::agents::Agent;
use goose::model::ModelConfig;
use goose::session::session_manager::SessionType;

pub struct AgentConfig {
    pub session_id: String,
    pub system_prompt: Option<String>,
}

pub async fn create_agent(config: AgentConfig) -> Result<Agent> {
    let provider_name = std::env::var("GOOSE_PROVIDER").unwrap_or_else(|_| "anthropic".into());
    let agent = Agent::new();

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let session = agent.config.session_manager
        .create_session(cwd, config.session_id, SessionType::User)
        .await
        .context("failed to create session")?;

    let provider = match std::env::var("GOOSE_MODEL") {
        Ok(model_name) => {
            let model_config = ModelConfig::new(&model_name)
                .context("invalid model config")?
                .with_canonical_limits(&provider_name);
            goose::providers::create(&provider_name, model_config, vec![]).await
        }
        Err(_) => goose::providers::create_with_default_model(&provider_name, vec![]).await,
    }.context("failed to create provider")?;

    agent.update_provider(provider, &session.id).await
        .context("failed to set provider")?;

    if let Some(prompt) = config.system_prompt {
        agent.extend_system_prompt("system".into(), prompt).await;
    }

    Ok(agent)
}
```

- [ ] **Step 3: Extract commands/board.rs and commands/rigs.rs**

Move `run_board_command()`, `show_board()` → `commands/board.rs`
Move `run_rigs_command()` → `commands/rigs.rs`

- [ ] **Step 4: Create commands/mod.rs**

```rust
pub mod board;
pub mod rigs;
```

- [ ] **Step 5: Slim down main.rs**

Main.rs keeps: `mod` declarations, `home_dir()`, `db_url()`, `main()`, and top-level wiring. Target: ~100-150 lines.

- [ ] **Step 6: Update evolver.rs to use runtime::create_agent**

Replace `create_evolver_agent()` with:
```rust
let agent = crate::runtime::create_agent(AgentConfig {
    session_id: "evolver".into(),
    system_prompt: Some(EVOLVER_SYSTEM_PROMPT.into()),
}).await?;
```

- [ ] **Step 7: Verify**

Run: `cargo test --workspace`
Expected: all PASS

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose/src/{cli.rs,runtime.rs,commands/,main.rs,evolver.rs}
git commit -m "refactor: split main.rs into cli, runtime, commands"
```

---

## Task 9: Split evolver.rs process_stamp

**Files:**
- Modify: `crates/opengoose/src/evolver.rs`

- [ ] **Step 1: Extract update_effectiveness()**

```rust
fn update_effectiveness(
    base_dir: &Path,
    stamp: &entity::stamp::Model,
    existing: &[LoadedSkill],
) -> anyhow::Result<()> {
    // ... move step 0 from process_stamp
}
```

- [ ] **Step 2: Extract prepare_context()**

```rust
struct StampContext {
    work_item: WorkItem,
    evolver_item_id: i64,
    log_summary: String,
    existing_pairs: Vec<(String, String)>,
    prompt: String,
}

async fn prepare_context(
    board: &Board,
    stamp: &entity::stamp::Model,
    existing: &[LoadedSkill],
) -> anyhow::Result<StampContext> {
    // ... move steps 1-6 from process_stamp
}
```

- [ ] **Step 3: Extract execute_action()**

```rust
async fn execute_action(
    base_dir: &Path,
    board: &Board,
    agent: &Agent,
    stamp: &entity::stamp::Model,
    ctx: &StampContext,
    existing: &[LoadedSkill],
) -> anyhow::Result<()> {
    // ... move steps 7-9 from process_stamp
}
```

- [ ] **Step 4: Rewrite process_stamp as composition**

```rust
async fn process_stamp(board: &Board, agent: &Agent, stamp: &entity::stamp::Model) -> anyhow::Result<()> {
    let base_dir = crate::home_dir();
    let existing = opengoose_skills::loader::load_skills(&base_dir, Some(&stamp.target_rig), None);

    update_effectiveness(&base_dir, stamp, &existing)?;
    let ctx = prepare_context(board, stamp, &existing).await?;
    execute_action(&base_dir, board, agent, stamp, &ctx, &existing).await?;

    board.submit(ctx.evolver_item_id, &RigId::new("evolver")).await?;
    Ok(())
}
```

- [ ] **Step 5: Verify**

Run: `cargo test --workspace`
Expected: all PASS

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose/src/evolver.rs
git commit -m "refactor: split process_stamp into update_effectiveness, prepare_context, execute_action"
```

---

## Task 10: Clean up binary crate ENV_LOCK and remaining duplicates

**Files:**
- Modify: `crates/opengoose/src/main.rs` — remove ENV_LOCK if no longer needed
- Modify: `crates/opengoose/src/web/api.rs` — use IsolatedEnv from test_utils or keep local
- Modify: `crates/opengoose/src/logs.rs` — update test env isolation

- [ ] **Step 1: Audit remaining ENV_LOCK usage in binary crate**

After Task 5 moved skill tests to opengoose-skills, check which files still use `ENV_LOCK`:
- `web/api.rs` tests (4 usages)
- `logs.rs` tests (2 usages)
- `main.rs` tests

These stay in the binary crate, so keep `ENV_LOCK` in main.rs for them.

- [ ] **Step 2: Remove any dead imports/re-exports**

Search for unused `pub use` or `mod` declarations in the binary crate's skill files.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: clean up remaining duplicates and dead imports"
```

---

## Task 11: Final verification

- [ ] **Step 1: Full test suite**

Run: `cargo test --workspace`
Expected: all PASS

- [ ] **Step 2: Clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: clean

- [ ] **Step 3: Manual TUI verification**

Run: `cargo run` (launches TUI)
Verify:
- Chat tab: Operator responds to messages
- Board tab: shows board state
- Logs tab: shows log entries

- [ ] **Step 4: Manual Web verification**

Run: `cargo run -- --port 1355`
Verify: `curl http://localhost:1355/api/board` returns JSON

- [ ] **Step 5: Manual Worker verification**

Run: `cargo run -- run "test task"`
Verify: Worker picks up and executes the task

- [ ] **Step 6: Final commit if any cleanup needed**

```bash
git add -A
git commit -m "refactor: final cleanup after Plan B restructure"
```
