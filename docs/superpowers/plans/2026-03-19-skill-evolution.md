# Skill Evolution System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace template-based skill generation with LLM-driven analysis, unify Board to async-only, add system rigs, and support 3-scope skill hierarchy with lifecycle management.

**Architecture:** DbBoard becomes the sole Board implementation (renamed). Evolver rig listens for low stamp events via stamp_notify, creates work items, and uses Goose agent.reply() for LLM analysis. Skills are stored in a 3-scope hierarchy (Global/Project/Rig) with Active→Dormant→Archived lifecycle.

**Tech Stack:** Rust, SeaORM (SQLite), Goose agent framework, tokio async runtime, JSONL conversation logs.

**Spec:** `docs/superpowers/specs/2026-03-19-skill-evolution-design.md`

---

## File Structure

### Removed files
- `crates/opengoose-board/src/board.rs` — in-memory Board
- `crates/opengoose-board/src/store.rs` — CowStore
- `crates/opengoose-board/src/branch.rs` — Branch
- `crates/opengoose-board/src/merge.rs` — merge logic

### Renamed
- `crates/opengoose-board/src/db_board.rs` → `crates/opengoose-board/src/board.rs` (DbBoard → Board)

### Modified
- `crates/opengoose-board/src/lib.rs` — remove dead modules, update re-exports
- `crates/opengoose-board/src/work_item.rs` — add `BoardError::SystemRigProtected`
- `crates/opengoose-board/src/entity/stamp.rs` — add `evolved_at` column
- `crates/opengoose-board/src/entity/mod.rs` — if exists, verify exports
- `crates/opengoose-rig/src/rig.rs` — `Arc<Mutex<Board>>` → `Arc<Board>`, async calls
- `crates/opengoose-rig/src/mcp_tools.rs` — `Arc<Mutex<Board>>` → `Arc<Board>`, remove `.lock().await`
- `crates/opengoose-rig/src/lib.rs` — add evolver module
- `crates/opengoose-rig/src/work_mode.rs` — add `EvolveMode`
- `crates/opengoose-rig/src/middleware.rs` — remove duplicated `parse_skill_header`, use load.rs
- `crates/opengoose/src/main.rs` — Evolver spawn, `--by` removal, system rig init
- `crates/opengoose/src/skills/mod.rs` — scope support
- `crates/opengoose/src/skills/load.rs` — 3-scope loading, catalog cap, `last_included_at` update
- `crates/opengoose/src/skills/evolve.rs` — LLM-based replacement
- `crates/opengoose/src/skills/list.rs` — scope + lifecycle status display
- `crates/opengoose/src/skills/add.rs` — `.goose/skills/` → `.opengoose/skills/` path

### New
- `crates/opengoose/src/evolver.rs` — Evolver run loop + lazy Agent init

---

## Task 1: Board Unification — Remove In-Memory, Rename, Drop Mutex

This is a single atomic task. All files must compile together.

**Files:**
- Delete: `crates/opengoose-board/src/board.rs` (in-memory Board)
- Delete: `crates/opengoose-board/src/store.rs` (CowStore)
- Delete: `crates/opengoose-board/src/branch.rs`
- Delete: `crates/opengoose-board/src/merge.rs`
- Rename: `crates/opengoose-board/src/db_board.rs` → `crates/opengoose-board/src/board.rs`
- Modify: `crates/opengoose-board/src/lib.rs`
- Modify: `crates/opengoose-rig/src/rig.rs` — `Arc<Board>` (no Mutex)
- Modify: `crates/opengoose-rig/src/mcp_tools.rs` — `Arc<Board>`, async calls
- Modify: `crates/opengoose/src/main.rs` — `DbBoard` → `Board`
- Modify: `crates/opengoose/src/web/mod.rs` — `DbBoard` → `Board`
- Modify: `crates/opengoose/src/web/api.rs` — `DbBoard` → `Board`
- Audit: `crates/opengoose/src/tui/` — check for `DbBoard` references

- [ ] **Step 1: Delete in-memory Board files and rename**

```bash
git rm crates/opengoose-board/src/board.rs
git rm crates/opengoose-board/src/store.rs
git rm crates/opengoose-board/src/branch.rs
git rm crates/opengoose-board/src/merge.rs
git mv crates/opengoose-board/src/db_board.rs crates/opengoose-board/src/board.rs
```

- [ ] **Step 2: In new board.rs, rename DbBoard → Board**

Search and replace within the file:
- `pub struct DbBoard` → `pub struct Board`
- `DbBoard::connect` → `Board::connect`
- `DbBoard::in_memory` → `Board::in_memory`
- All test references: `DbBoard` → `Board`
- All doc comments mentioning `DbBoard`

- [ ] **Step 3: Update lib.rs**

```rust
// crates/opengoose-board/src/lib.rs
pub mod entity;
pub mod board;      // was db_board
pub mod beads;
pub mod relations;
pub mod stamps;
pub mod work_item;

// Re-exports
pub use board::Board;
pub use stamps::{Stamp, StampStore, TrustLevel};
pub use work_item::{BoardError, PostWorkItem, Priority, RigId, Status, WorkItem};
```

Remove: `mod store`, `mod branch`, `mod merge`, `mod db_board`, `pub use store::CowStore`.

- [ ] **Step 4: Fix all `DbBoard` references across workspace**

Search and replace across all crates:
- `crates/opengoose/src/main.rs` — `use opengoose_board::db_board::DbBoard` → `use opengoose_board::Board`
- `crates/opengoose/src/web/mod.rs` — `DbBoard` → `Board`
- `crates/opengoose/src/web/api.rs` — `DbBoard` → `Board`
- `crates/opengoose/src/tui/` — audit all files for `DbBoard` references
- `crates/opengoose/src/main.rs` helper functions (`show_board`, `run_board_command`, etc.) — `&DbBoard` → `&Board`

- [ ] **Step 5: Update Rig<M> — Arc<Board> without Mutex**

In `crates/opengoose-rig/src/rig.rs`:
- `board: Option<Arc<Mutex<Board>>>` → `board: Option<Arc<Board>>`
- `new()` parameter: `Arc<Mutex<Board>>` → `Arc<Board>`
- Remove `use tokio::sync::Mutex`
- `Worker::run()` — `board.lock().await` for `notify_handle()` → `board.notify_handle()`
- `Worker::try_claim_and_execute()` — remove `.lock().await`, use async Board methods directly:
  ```rust
  let ready = board_arc.ready().await?;
  let item = board_arc.claim(item.id, &self.id).await?;
  board_arc.submit(item.id, &self.id).await?;
  ```

- [ ] **Step 6: Update BoardClient in mcp_tools.rs**

- `board: Arc<Mutex<Board>>` → `board: Arc<Board>`
- Constructor parameter: same change
- All 4 handler methods: remove `self.board.lock().await`, use `self.board.method().await` directly
  - `handle_read_board`: `self.board.list().await`
  - `handle_claim_next`: `self.board.ready().await`, `self.board.claim().await`
  - `handle_submit`: `self.board.submit().await`
  - `handle_create_task`: `self.board.post().await`

- [ ] **Step 7: Build and verify**

```bash
cargo build 2>&1 | head -40
```

Expected: 0 errors. All crates compile.

- [ ] **Step 8: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add crates/opengoose-board/ crates/opengoose-rig/src/rig.rs crates/opengoose-rig/src/mcp_tools.rs crates/opengoose/src/main.rs crates/opengoose/src/web/ crates/opengoose/src/tui/
git commit -m "refactor: unify Board — remove in-memory Board, rename DbBoard → Board, drop Mutex"
```

---

## Task 2: System Rig + CLI --by Removal

**Files:**
- Modify: `crates/opengoose-board/src/work_item.rs` — add `BoardError::SystemRigProtected`
- Modify: `crates/opengoose-board/src/board.rs` — `ensure_system_rigs()`, protect `remove_rig()`
- Modify: `crates/opengoose/src/main.rs` — remove `--by`, `stamped_by = "human"`

- [ ] **Step 1: Write test for system rig protection**

In `crates/opengoose-board/src/board.rs` tests section:

```rust
#[tokio::test]
async fn system_rigs_created_on_connect() {
    let board = Board::in_memory().await.unwrap();
    let human = board.get_rig("human").await.unwrap();
    assert!(human.is_some());
    assert_eq!(human.unwrap().rig_type, "system");

    let evolver = board.get_rig("evolver").await.unwrap();
    assert!(evolver.is_some());
    assert_eq!(evolver.unwrap().rig_type, "system");
}

#[tokio::test]
async fn cannot_remove_system_rig() {
    let board = Board::in_memory().await.unwrap();
    let result = board.remove_rig("human").await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p opengoose-board system_rig 2>&1
```

Expected: FAIL — `SystemRigProtected` variant doesn't exist, `ensure_system_rigs` not called.

- [ ] **Step 3: Add `BoardError::SystemRigProtected`**

In `crates/opengoose-board/src/work_item.rs`, add to the `BoardError` enum:

```rust
#[error("cannot remove system rig: {0}")]
SystemRigProtected(String),
```

- [ ] **Step 4: Implement `ensure_system_rigs()`**

In `crates/opengoose-board/src/board.rs`, add after `create_tables()`:

```rust
async fn ensure_system_rigs(db: &DatabaseConnection) -> Result<(), BoardError> {
    for (id, rig_type) in [("human", "system"), ("evolver", "system")] {
        let existing = entity::rig::Entity::find_by_id(id.to_string())
            .one(db)
            .await
            .map_err(db_err)?;
        if existing.is_none() {
            entity::rig::Entity::insert(entity::rig::ActiveModel {
                id: Set(id.to_string()),
                rig_type: Set(rig_type.to_string()),
                recipe: Set(None),
                tags: Set(None),
                created_at: Set(chrono::Utc::now()),
            })
            .exec(db)
            .await
            .map_err(db_err)?;
        }
    }
    Ok(())
}
```

Call it from `connect()` after `create_tables()`:

```rust
pub async fn connect(db_url: &str) -> Result<Self, BoardError> {
    let db = Database::connect(db_url).await.map_err(db_err)?;
    Self::create_tables(&db).await?;
    Self::ensure_system_rigs(&db).await?;
    // ...
}
```

- [ ] **Step 5: Protect `remove_rig()`**

In `remove_rig()`:

```rust
pub async fn remove_rig(&self, id: &str) -> Result<(), BoardError> {
    if let Some(rig) = self.get_rig(id).await? {
        if rig.rig_type == "system" {
            return Err(BoardError::SystemRigProtected(id.to_string()));
        }
    }
    entity::rig::Entity::delete_by_id(id.to_string())
        .exec(&self.db)
        .await
        .map_err(db_err)?;
    Ok(())
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p opengoose-board system_rig 2>&1
```

Expected: PASS.

- [ ] **Step 7: Remove `--by` from CLI**

In `crates/opengoose/src/main.rs`:

Remove `#[arg(long)] by: String` from `BoardAction::Stamp`.
Remove `by` from the match arm.
Hardcode `stamped_by = "human"` in the stamp handler.

- [ ] **Step 8: Build and test**

```bash
cargo build && cargo test --workspace 2>&1 | tail -10
```

- [ ] **Step 9: Commit**

```bash
git add crates/opengoose-board/src/board.rs crates/opengoose-board/src/work_item.rs crates/opengoose/src/main.rs
git commit -m "feat: system rigs (human, evolver) — auto-created, deletion-protected, CLI --by removed"
```

---

## Task 3: stamp_notify + evolved_at

**Files:**
- Modify: `crates/opengoose-board/src/entity/stamp.rs` — add `evolved_at`
- Modify: `crates/opengoose-board/src/board.rs` — `stamp_notify`, `unprocessed_low_stamps()`, `mark_stamp_evolved()`

- [ ] **Step 1: Write test for stamp_notify**

```rust
#[tokio::test]
async fn stamp_notify_fires_on_add_stamp() {
    let board = Board::in_memory().await.unwrap();
    let notify = board.stamp_notify_handle();
    let item = board.post(post_req("test")).await.unwrap();

    let handle = tokio::spawn(async move {
        notify.notified().await;
        true
    });

    tokio::task::yield_now().await;
    board.add_stamp("rig-a", item.id, "Quality", 0.5, "Leaf", "human", None).await.unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn unprocessed_low_stamps_returns_only_unevolved() {
    let board = Board::in_memory().await.unwrap();
    let item = board.post(post_req("test")).await.unwrap();

    let id1 = board.add_stamp("rig-a", item.id, "Quality", 0.2, "Leaf", "human", None).await.unwrap();
    let id2 = board.add_stamp("rig-a", item.id, "Reliability", 0.8, "Leaf", "human", None).await.unwrap();

    let low = board.unprocessed_low_stamps(0.3).await.unwrap();
    assert_eq!(low.len(), 1);
    assert_eq!(low[0].id, id1);

    board.mark_stamp_evolved(id1).await.unwrap();
    let low = board.unprocessed_low_stamps(0.3).await.unwrap();
    assert!(low.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p opengoose-board stamp_notify unprocessed 2>&1
```

Expected: FAIL.

- [ ] **Step 3: Add `evolved_at` to stamp entity**

In `crates/opengoose-board/src/entity/stamp.rs`:

```rust
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub target_rig: String,
    pub work_item_id: i64,
    pub dimension: String,
    pub score: f32,
    pub severity: String,
    pub stamped_by: String,
    pub comment: Option<String>,
    pub evolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

- [ ] **Step 4: Add `stamp_notify` to Board**

```rust
pub struct Board {
    db: DatabaseConnection,
    notify: Arc<Notify>,
    stamp_notify: Arc<Notify>,
}
```

Update `connect()` and `in_memory()` to initialize `stamp_notify`.

Add `stamp_notify.notify_waiters()` at the end of `add_stamp()`.

Add `stamp_notify_handle()`:
```rust
pub fn stamp_notify_handle(&self) -> Arc<Notify> {
    Arc::clone(&self.stamp_notify)
}
```

- [ ] **Step 5: Add `unprocessed_low_stamps()` and `mark_stamp_evolved()`**

```rust
pub async fn unprocessed_low_stamps(&self, threshold: f32) -> Result<Vec<entity::stamp::Model>, BoardError> {
    entity::stamp::Entity::find()
        .filter(entity::stamp::Column::Score.lt(threshold))
        .filter(entity::stamp::Column::EvolvedAt.is_null())
        .all(&self.db)
        .await
        .map_err(db_err)
}

pub async fn mark_stamp_evolved(&self, stamp_id: i64) -> Result<bool, BoardError> {
    let result = entity::stamp::Entity::update_many()
        .col_expr(entity::stamp::Column::EvolvedAt, Expr::value(chrono::Utc::now()))
        .filter(entity::stamp::Column::Id.eq(stamp_id))
        .filter(entity::stamp::Column::EvolvedAt.is_null())
        .exec(&self.db)
        .await
        .map_err(db_err)?;
    Ok(result.rows_affected > 0)
}
```

- [ ] **Step 6: Update `add_stamp()` — set `evolved_at: NotSet`**

In the `ActiveModel` construction, add `evolved_at: NotSet`.

- [ ] **Step 7: Run tests**

```bash
cargo test -p opengoose-board 2>&1 | tail -20
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose-board/src/board.rs crates/opengoose-board/src/entity/stamp.rs
git commit -m "feat: stamp_notify + evolved_at — Evolver trigger mechanism"
```

---

## Task 4: EvolveMode + Evolver Type

**Files:**
- Modify: `crates/opengoose-rig/src/work_mode.rs` — add `EvolveMode`
- Modify: `crates/opengoose-rig/src/rig.rs` — add `Evolver` type alias

- [ ] **Step 1: Write test for EvolveMode**

In `crates/opengoose-rig/src/work_mode.rs` tests:

```rust
#[test]
fn evolve_mode_returns_stamp_based_session() {
    let mode = EvolveMode;
    let a = mode.session_for(&WorkInput::task("analyze stamp", 5));
    assert_eq!(a, "evolve-5");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p opengoose-rig evolve_mode 2>&1
```

- [ ] **Step 3: Implement EvolveMode**

In `work_mode.rs`:

```rust
/// Evolver용: stamp 분석당 세션. 대화 캐시 오염 방지.
pub struct EvolveMode;

impl WorkMode for EvolveMode {
    fn session_for(&self, input: &WorkInput) -> String {
        match input.work_id {
            Some(id) => format!("evolve-{id}"),
            None => format!(
                "evolve-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            ),
        }
    }
}
```

- [ ] **Step 4: Add Evolver type alias**

In `crates/opengoose-rig/src/rig.rs`:

```rust
/// Evolver: stamp 감지 → 스킬 생성. 분석당 세션.
pub type Evolver = Rig<EvolveMode>;
```

Add `EvolveMode` to the import from `work_mode`.

- [ ] **Step 5: Run tests**

```bash
cargo test -p opengoose-rig 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose-rig/src/work_mode.rs crates/opengoose-rig/src/rig.rs
git commit -m "feat: EvolveMode + Evolver type alias — stamp-per-session strategy"
```

---

## Task 5: Skill Scopes — Directory Structure + 3-Scope Loading

**Files:**
- Modify: `crates/opengoose/src/skills/load.rs` — 3-scope loading, catalog cap
- Modify: `crates/opengoose/src/skills/list.rs` — scope + status display
- Modify: `crates/opengoose/src/skills/add.rs` — `.opengoose/skills/` path
- Modify: `crates/opengoose/src/skills/mod.rs` — scope support
- Modify: `crates/opengoose-rig/src/middleware.rs` — use load.rs, remove duplicate

- [ ] **Step 1: Write test for 3-scope loading**

In `crates/opengoose/src/skills/load.rs` tests:

```rust
#[test]
fn load_skills_3_scope() {
    let tmp = tempfile::tempdir().unwrap();

    // Global
    let global = tmp.path().join("global/installed/skill-a");
    std::fs::create_dir_all(&global).unwrap();
    std::fs::write(global.join("SKILL.md"), "---\nname: skill-a\ndescription: Global skill\n---\n").unwrap();

    // Rig
    let rig = tmp.path().join("rigs/worker-1/skills/learned/skill-b");
    std::fs::create_dir_all(&rig).unwrap();
    std::fs::write(rig.join("SKILL.md"), "---\nname: skill-b\ndescription: Rig skill\n---\n").unwrap();

    let skills = load_skills_3_scope(
        &tmp.path().join("global"),
        None, // no project
        Some("worker-1"),
        &tmp.path().join("rigs"),
    );
    assert_eq!(skills.len(), 2);
}

#[test]
fn catalog_respects_cap() {
    let mut skills = Vec::new();
    for i in 0..15 {
        skills.push(LoadedSkill {
            name: format!("skill-{i}"),
            description: format!("Skill {i}"),
            path: PathBuf::from(format!("/tmp/skill-{i}")),
            content: String::new(),
            scope: SkillScope::Learned,
        });
    }
    let catalog = build_catalog_capped(&skills, 10);
    // Should have at most 10 entries
    assert!(catalog.matches("- **").count() <= 10);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p opengoose load_skills_3_scope catalog_respects 2>&1
```

- [ ] **Step 3: Implement 3-scope loading**

Add `SkillScope` enum and `load_skills_3_scope()` function to `load.rs`. Add `build_catalog_capped()` with max 10 skills, installed first.

Update `metadata.json` with `last_included_at` timestamp on each catalog build.

- [ ] **Step 4: Consolidate `parse_skill_header` — keep in opengoose-rig, reuse from load.rs**

Since `opengoose-rig` can't depend on `opengoose`, keep `parse_skill_header` in `middleware.rs` as the canonical version. In `load.rs`, import it via `opengoose_rig::middleware::parse_skill_header` (opengoose already depends on opengoose-rig). Make the function `pub` in middleware.rs. Remove the duplicate from load.rs.

- [ ] **Step 5: Update `list.rs` for scope display**

Show skills grouped by scope with lifecycle status:

```
Global (installed):
  skill-a — Global skill                          (installed)
Rig worker-1 (learned):
  test-before-submit — Use when modifying code...  (active)
```

- [ ] **Step 6: Update `add.rs` path**

Change `.goose/skills/` references to `.opengoose/skills/installed/`.

- [ ] **Step 7: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

- [ ] **Step 8: Commit**

```bash
git add crates/opengoose/src/skills/ crates/opengoose-rig/src/middleware.rs
git commit -m "feat: 3-scope skill hierarchy — Global/Project/Rig with catalog cap"
```

---

## Task 6: LLM-Based Skill Generation (evolve.rs rewrite)

**Files:**
- Rewrite: `crates/opengoose/src/skills/evolve.rs` — LLM-based analysis
- Modify: `crates/opengoose/src/main.rs` — remove inline evolve trigger from stamp handler

- [ ] **Step 1: Write test for skill output validation**

```rust
#[test]
fn validate_skill_output_valid() {
    let output = "---\nname: test-skill\ndescription: Use when testing\n---\n# Body\n";
    let result = validate_skill_output(output);
    assert!(result.is_ok());
}

#[test]
fn validate_skill_output_missing_frontmatter() {
    let output = "# No frontmatter\nJust text.";
    let result = validate_skill_output(output);
    assert!(result.is_err());
}

#[test]
fn validate_skill_output_bad_description() {
    let output = "---\nname: test\ndescription: This skill does things\n---\n";
    let result = validate_skill_output(output);
    assert!(result.is_err()); // doesn't start with "Use when"
}

#[test]
fn parse_llm_response_skip() {
    let response = "SKIP";
    let result = parse_evolve_response(response);
    assert_eq!(result, EvolveAction::Skip);
}

#[test]
fn parse_llm_response_update() {
    let response = "UPDATE:existing-skill";
    let result = parse_evolve_response(response);
    assert_eq!(result, EvolveAction::Update("existing-skill".into()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p opengoose validate_skill parse_evolve 2>&1
```

- [ ] **Step 3: Implement validation + response parsing**

In `evolve.rs`:

```rust
pub enum EvolveAction {
    Create(String),  // valid SKILL.md content
    Update(String),  // existing skill name to update
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

pub fn validate_skill_output(content: &str) -> anyhow::Result<()> {
    // Check frontmatter exists
    // Check name: lowercase + hyphens
    // Check description starts with "Use when"
}
```

- [ ] **Step 4: Implement `build_evolve_prompt()`**

```rust
pub fn build_evolve_prompt(
    dimension: &str,
    score: f32,
    comment: Option<&str>,
    work_item_title: &str,
    work_item_id: i64,
    log_summary: &str,
    existing_skills: &[(String, String)], // (name, description)
) -> String {
    // Format the prompt as specified in the design spec
}
```

- [ ] **Step 5: Implement `write_skill_to_rig_scope()`**

```rust
pub fn write_skill_to_rig_scope(
    rig_id: &str,
    skill_content: &str,
    stamp_id: i64,
    work_item_id: i64,
    dimension: &str,
    score: f32,
) -> anyhow::Result<String> {
    // Parse name from frontmatter
    // Write to ~/.opengoose/rigs/{rig_id}/skills/learned/{name}/SKILL.md
    // Write metadata.json with generated_from + effectiveness stub
    // Return skill name
}
```

- [ ] **Step 6: Implement `read_conversation_log()` for LLM context**

```rust
pub fn read_conversation_log(work_item_id: i64) -> String {
    let session_id = format!("task-{work_item_id}");
    opengoose_rig::conversation_log::read_log(&session_id)
        .map(|content| summarize_for_prompt(&content, 2000))
        .unwrap_or_default()
}

fn summarize_for_prompt(content: &str, max_chars: usize) -> String {
    // Take last N chars of log, focusing on error/failure lines
    // Truncate to max_chars
}
```

- [ ] **Step 7: Add validation retry logic**

```rust
pub fn validate_and_retry(content: &str) -> anyhow::Result<String> {
    match validate_skill_output(content) {
        Ok(()) => Ok(content.to_string()),
        Err(e) => Err(anyhow::anyhow!("validation failed: {e} — retry with format fix"))
    }
}
```

The Evolver loop (Task 7) will call this, retry once with "fix the format" appended, and skip on 2nd failure.

- [ ] **Step 8: Run tests**

```bash
cargo test -p opengoose validate_skill parse_evolve build_evolve write_skill 2>&1
```

- [ ] **Step 9: Commit**

```bash
git add crates/opengoose/src/skills/evolve.rs
git commit -m "feat: LLM-based skill generation — validation, prompt builder, rig-scope writer"
```

---

## Task 7: Evolver Run Loop

**Files:**
- Create: `crates/opengoose/src/evolver.rs` — main Evolver loop
- Modify: `crates/opengoose/src/main.rs` — spawn Evolver

- [ ] **Step 1: Create evolver.rs with run loop**

```rust
// crates/opengoose/src/evolver.rs

use opengoose_board::Board;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{info, warn};

/// Evolver run loop. Lazy-inits Agent on first stamp event.
pub async fn run(board: Arc<Board>, stamp_notify: Arc<Notify>) {
    info!("evolver: waiting for stamp events");

    let mut agent = None; // lazy init

    loop {
        // Wait for stamp_notify OR 5-minute fallback sweep
        tokio::select! {
            _ = stamp_notify.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(300)) => {}
        }

        // Query unprocessed low stamps
        let stamps = match board.unprocessed_low_stamps(0.3).await {
            Ok(s) => s,
            Err(e) => {
                warn!("evolver: failed to query stamps: {e}");
                continue;
            }
        };

        if stamps.is_empty() {
            continue;
        }

        // Lazy init Agent on first real work
        if agent.is_none() {
            match create_evolver_agent().await {
                Ok(a) => agent = Some(a),
                Err(e) => {
                    warn!("evolver: failed to create agent: {e}");
                    continue;
                }
            }
        }

        for stamp in &stamps {
            // Atomically mark as evolved
            match board.mark_stamp_evolved(stamp.id).await {
                Ok(true) => {}
                Ok(false) => continue, // another Evolver got it
                Err(e) => {
                    warn!("evolver: failed to mark stamp {}: {e}", stamp.id);
                    continue;
                }
            }

            if let Err(e) = process_stamp(&board, agent.as_ref().unwrap(), stamp).await {
                warn!("evolver: failed to process stamp {}: {e}", stamp.id);
            }
        }
    }
}
```

- [ ] **Step 2: Implement `create_evolver_agent()`**

Create a Goose Agent with the Evolver-specific system prompt (from the spec).

- [ ] **Step 3: Implement `process_stamp()`**

1. Post "skill generation" work item to Board (`created_by: "evolver"`)
2. Claim it
3. Read conversation log via `evolve::read_conversation_log(work_item_id)`
4. Load existing skills list for dedup check
5. Build prompt via `evolve::build_evolve_prompt()`
6. Call agent.reply() → collect response text
7. Parse response via `evolve::parse_evolve_response()`:
   - SKIP → submit work item, continue
   - UPDATE:name → update existing skill, submit
   - Create → validate, retry once on failure, write to rig scope, submit
8. On 2nd validation failure → mark work item stuck, log error

- [ ] **Step 4: Remove old template-based evolve trigger from main.rs**

Remove the inline `skills::evolve::try_evolve_skill()` call and surrounding `low_scores` logic from the stamp handler. The Evolver loop handles this now.

- [ ] **Step 5: Wire Evolver into main.rs**

```rust
// In main() None branch (TUI mode):
let stamp_notify = board.stamp_notify_handle();
tokio::spawn(crate::evolver::run(Arc::clone(&board), stamp_notify));
```

Add `mod evolver;` to main.rs.

- [ ] **Step 6: Build**

```bash
cargo build 2>&1 | head -20
```

- [ ] **Step 7: Commit**

```bash
git add crates/opengoose/src/evolver.rs crates/opengoose/src/main.rs
git commit -m "feat: Evolver run loop — lazy init, stamp_notify + fallback sweep, LLM analysis"
```

---

## Task 8: Skill Lifecycle (Active/Dormant/Archived)

**Files:**
- Modify: `crates/opengoose/src/skills/load.rs` — lifecycle logic
- Modify: `crates/opengoose/src/skills/list.rs` — status display

- [ ] **Step 1: Write test for lifecycle determination**

```rust
#[test]
fn skill_lifecycle_active_when_recent() {
    let meta = SkillMetadata {
        generated_at: Utc::now().to_rfc3339(),
        last_included_at: Some(Utc::now().to_rfc3339()),
        // ...
    };
    assert_eq!(determine_lifecycle(&meta), Lifecycle::Active);
}

#[test]
fn skill_lifecycle_dormant_after_30_days() {
    let old = Utc::now() - chrono::Duration::days(35);
    let meta = SkillMetadata {
        generated_at: old.to_rfc3339(),
        last_included_at: Some(old.to_rfc3339()),
        // ...
    };
    assert_eq!(determine_lifecycle(&meta), Lifecycle::Dormant);
}
```

- [ ] **Step 2: Implement lifecycle logic**

```rust
pub enum Lifecycle {
    Active,
    Dormant,
    Archived,
}

pub fn determine_lifecycle(meta: &SkillMetadata) -> Lifecycle {
    let last = meta.last_included_at
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| {
            DateTime::parse_from_rfc3339(&meta.generated_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now())
        });

    let days = (Utc::now() - last).num_days();
    if days <= 30 { Lifecycle::Active }
    else if days <= 120 { Lifecycle::Dormant }
    else { Lifecycle::Archived }
}
```

- [ ] **Step 3: Update `build_catalog_capped()` to skip Dormant/Archived**

Only include Active learned skills + all installed skills.
Update `last_included_at` in metadata.json for included skills.

- [ ] **Step 4: Add `--archived` flag to skills list**

- [ ] **Step 5: Run tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/opengoose/src/skills/load.rs crates/opengoose/src/skills/list.rs
git commit -m "feat: skill lifecycle — Active/Dormant/Archived with 30/120-day thresholds"
```

---

## Task 9: Effectiveness Tracking

**Files:**
- Modify: `crates/opengoose/src/skills/evolve.rs` — metadata with effectiveness
- Modify: `crates/opengoose/src/evolver.rs` — track subsequent scores

- [ ] **Step 1: Write test for effectiveness update**

```rust
#[test]
fn update_effectiveness_adds_score() {
    let tmp = tempfile::tempdir().unwrap();
    let skill_dir = tmp.path().join("test-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();

    let meta = SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id: 1, work_item_id: 1,
            dimension: "Quality".into(), score: 0.2,
        },
        generated_at: Utc::now().to_rfc3339(),
        last_included_at: None,
        evolver_work_item_id: None,
        effectiveness: Effectiveness {
            injected_count: 0,
            subsequent_scores: vec![],
        },
    };
    let json = serde_json::to_string_pretty(&meta).unwrap();
    std::fs::write(skill_dir.join("metadata.json"), &json).unwrap();

    update_effectiveness(&skill_dir, 0.7).unwrap();

    let updated: SkillMetadata = serde_json::from_str(
        &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap()
    ).unwrap();
    assert_eq!(updated.effectiveness.subsequent_scores, vec![0.7]);
}
```

- [ ] **Step 2: Implement `update_effectiveness()`**

Read metadata.json, append score, write back.

- [ ] **Step 3: Wire into Evolver loop**

After a stamp arrives, check if any existing skill's `generated_from.dimension` matches the stamp's dimension and `generated_from.work_item_id`'s target rig matches. If so, update that skill's `subsequent_scores`.

- [ ] **Step 4: Run tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/skills/evolve.rs crates/opengoose/src/evolver.rs
git commit -m "feat: skill effectiveness tracking — subsequent_scores in metadata.json"
```

---

## Task 10: Final Integration Test + Cleanup

**Files:**
- `crates/opengoose/src/skills/discover.rs` — `.goose/skills/` path references
- `crates/opengoose/src/skills/add.rs` — `.goose/skills/` path references
- `crates/opengoose/src/skills/list.rs` — `.goose/skills/` path references
- `crates/opengoose/src/skills/load.rs` — effectiveness judgment logic

- [ ] **Step 1: Fix ALL remaining `.goose/skills/` references**

Consolidate path migration:
- `discover.rs` line 24: add `.opengoose/skills/installed`, `.opengoose/skills/learned` to standard_dirs
- `discover.rs` tests: update `.goose/skills/` references
- `add.rs` `install_base()`: `.goose/skills` → `.opengoose/skills/installed`
- `list.rs` line 9: `.goose/skills` → `.opengoose/skills`

- [ ] **Step 2: Add effectiveness judgment logic**

In `load.rs`, add:

```rust
pub fn is_effective(meta: &SkillMetadata) -> Option<bool> {
    let scores = &meta.effectiveness.subsequent_scores;
    if scores.len() < 3 {
        return None; // not enough data
    }
    let avg: f32 = scores.iter().sum::<f32>() / scores.len() as f32;
    let improvement = avg - meta.generated_from.score;
    Some(improvement >= 0.2)
}
```

Use in `determine_lifecycle()`: ineffective skills decay faster (treat as 60+ days old).

- [ ] **Step 2: Full build**

```bash
cargo build 2>&1
```

Expected: 0 errors, warnings only for unused code (if any).

- [ ] **Step 3: Full test suite**

```bash
cargo test --workspace 2>&1
```

Expected: all tests pass.

- [ ] **Step 4: Manual smoke test**

```bash
# Start opengoose (Evolver should spawn)
# In another terminal:
opengoose rigs                        # shows human, evolver
opengoose board create "Test task"
opengoose board claim 1
opengoose board submit 1
opengoose board stamp 1 -q 0.2 -r 0.8 -p 0.7 --comment "no tests"
# Wait for Evolver to process
opengoose skills list                 # should show generated skill
```

- [ ] **Step 5: Commit**

```bash
git add crates/opengoose/src/skills/
git commit -m "chore: integration cleanup — path migration, effectiveness judgment, final verification"
```
