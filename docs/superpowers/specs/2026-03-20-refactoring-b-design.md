# OpenGoose v0.2 Refactoring — Plan B (4-Crate)

## Goal

Resolve code quality issues: God Object (Board 34 methods), large files (evolve.rs 927, api.rs 922, load.rs 820, main.rs 705), duplicate code (6 patterns), and tight coupling (Board.db() exposed, skills trapped in binary crate).

All existing features preserved. TUI + Web + Worker run in single process.

## Verification Criteria

- `cargo test` all pass
- `cargo clippy` no warnings
- Manual: TUI launch, Operator chat, Worker pull loop, Web server responds

---

## 1. Crate Structure

### Before

```
opengoose-board  <-  opengoose-rig  <-  opengoose (binary)
                                          ^
                                     skills/ trapped here
```

### After

```
opengoose-board  <-  opengoose-skills  <-  opengoose-rig  <-  opengoose (binary)
                 ^
opengoose-board -+
```

| Crate | Responsibility | Dependencies |
|-------|---------------|--------------|
| `opengoose-board` | Board, WorkItem, Stamp, Rig, Relations | sea-orm, tokio, chrono, serde |
| `opengoose-skills` | Skill loading, evolution, metadata, file I/O, validation | serde, chrono, anyhow (NO board, NO rig) |
| `opengoose-rig` | Rig\<WorkMode\>, Operator/Worker, BoardClient, conversation log | board, skills, goose |
| `opengoose` | CLI + TUI + Web + Evolver loop | board, skills, rig, goose |

Key: `opengoose-skills` has NO dependency on board or rig. `read_conversation_log()` currently calls `opengoose_rig::conversation_log::read_log()` — this will be changed so the caller (evolver) passes the log content in, not skills pulling it.

---

## 2. Board God Object Decomposition

### Before: board.rs (1095 lines, 34 methods)

### After: impl Board split across files

```
opengoose-board/src/
├── board.rs         — Board struct, connect(), in_memory(), create_tables(), ensure_columns(), notify fields
├── work_items.rs    — impl Board: post, claim, submit, unclaim, mark_stuck, retry, abandon, get, list, ready, claimed_by
├── rigs.rs          — impl Board: register_rig, list_rigs, get_rig, remove_rig, ensure_system_rigs
├── stamps.rs        — impl Board: add_stamp, weighted_score, trust_level, unprocessed_low_stamps, recent_low_stamps, mark_stamp_evolved, stamps_for_item, stamps_for_rig (NEW)
├── relations.rs     — existing (unchanged)
├── entity/          — existing (unchanged)
├── work_item.rs     — existing (unchanged)
└── beads.rs         — existing (unchanged)
```

Changes:
- `Board.db()` removed from public API
- New query method: `Board::stamps_for_rig(rig_id)` returns domain types, not entities
- Stamp decay formula lives only in `stamps.rs::weighted_score()` — single source of truth
- Each file is an `impl Board` block (Rust allows this across files in the same crate)

---

## 3. opengoose-skills Crate Internal Structure

```
crates/opengoose-skills/src/
├── lib.rs              — pub mod re-exports
│
├── catalog.rs          — build_catalog_capped(), prompt injection catalog generation
├── lifecycle.rs        — Active/Dormant/Archived determination, last_included_at update
├── loader.rs           — 3-scope filesystem scan, LoadedSkill, collect_all_skills, scan_skill_dirs()
├── metadata.rs         — SkillMetadata, Effectiveness, GeneratedFrom, SkillFrontmatter,
│                         parse_frontmatter(), read_metadata(), write_metadata(), is_effective()
│
├── evolution/
│   ├── mod.rs          — pub re-exports
│   ├── parser.rs       — EvolveAction, SweepDecision, parse_evolve_response(), parse_sweep_response()
│   ├── validator.rs    — validate_skill_output() (uses parse_frontmatter internally)
│   ├── prompts.rs      — build_evolve_prompt(), build_update_prompt(), build_sweep_prompt(), summarize_for_prompt()
│   └── writer.rs       — write_skill_to_rig_scope(), update_existing_skill(), refine_skill(),
│                         update_effectiveness_versioned(), build_active_versions_json()
│
├── manage/
│   ├── mod.rs
│   ├── add.rs          — clone + install
│   ├── remove.rs       — delete
│   ├── update.rs       — re-clone sources
│   ├── promote.rs      — rig -> project/global promotion
│   ├── discover.rs     — Git repo scan
│   ├── list.rs         — display with lifecycle info
│   └── lock.rs         — version lock
│
├── source.rs           — Git URL parsing (unchanged)
│
└── test_utils.rs       — IsolatedEnv (RAII Drop guard), skill_path() helper
```

### Migration mapping

| Current location | New location | Change |
|-----------------|-------------|--------|
| `evolve.rs` lines 25-83 (parsing) | `evolution/parser.rs` | As-is |
| `evolve.rs` lines 89-128 (validation) | `evolution/validator.rs` | Uses shared parse_frontmatter() |
| `evolve.rs` lines 134-201 (prompts) | `evolution/prompts.rs` | `read_conversation_log()` removed, caller passes log |
| `evolve.rs` lines 259-366 (file I/O) | `evolution/writer.rs` | `home_dir()` dep removed, paths passed as args |
| `evolve.rs` lines 226-253 (types) | `metadata.rs` | Shared across crate |
| `evolve.rs` lines 423-471 (effectiveness) | `evolution/writer.rs` | Uses metadata.rs read/write |
| `load.rs` lines 72-129 (scan) | `loader.rs` | |
| `load.rs` lines 28-46 (lifecycle) | `lifecycle.rs` | |
| `load.rs` lines 208-260 (catalog) | `catalog.rs` | |
| `load.rs` lines 279-287 (effectiveness) | `metadata.rs` | |
| promote/remove/update test setup | `test_utils.rs` | 3 duplicates -> 1 |

### What remains in binary crate

```
opengoose/src/skills/
└── mod.rs    — SkillsAction CLI enum + dispatch to opengoose_skills functions
```

---

## 4. Binary Crate (opengoose) Cleanup

### Before: main.rs (705 lines) doing everything

### After

```
crates/opengoose/src/
├── main.rs              — CLI parse + match dispatch (~100 lines)
├── cli.rs               — Cli, Commands, BoardAction, etc. (clap derives)
├── runtime.rs           — create_agent(AgentConfig), init runtime
│
├── commands/
│   ├── mod.rs
│   ├── board.rs         — run_board_command(), show_board()
│   ├── rigs.rs          — run_rigs_command()
│   ├── skills.rs        — CLI dispatch (calls opengoose_skills)
│   └── logs.rs          — log management
│
├── evolver.rs           — run(), process_stamp() split into 3 functions
├── tui/                 — unchanged
└── web/
    └── api.rs           — Board.db() access removed, uses query methods
```

### evolver.rs process_stamp() split

```rust
// Before: 182-line monolith
// After: 3 focused functions

async fn process_stamp(board, agent, stamp) -> Result<()> {
    update_effectiveness(board, stamp)?;              // step 0
    let ctx = prepare_context(board, stamp).await?;   // steps 1-6
    execute_action(board, agent, &ctx).await?;         // steps 7-9
}
```

### Agent creation dedup

```rust
// Before: create_base_agent() in main.rs + create_evolver_agent() in evolver.rs
// After: single function in runtime.rs

pub struct AgentConfig {
    pub session_id: String,
    pub system_prompt: Option<String>,
}

pub async fn create_agent(config: AgentConfig) -> Result<Agent> { ... }
```

### web/api.rs coupling fix

```rust
// Before: entity::stamp::Entity::find()...all(state.board.db())
// After:  state.board.stamps_for_rig(&id).await?
```

---

## 5. Duplicate Code Elimination

| Pattern | Current duplicates | After | Single source |
|---------|-------------------|-------|---------------|
| Stamp decay formula | 2 (board + web/api.rs) | 1 | `board/stamps.rs::weighted_score()` |
| Agent creation | 2 (main.rs + evolver.rs) | 1 | `runtime.rs::create_agent()` |
| Test env setup | 3 (promote + remove + update) | 1 | `skills/test_utils.rs::IsolatedEnv` |
| Frontmatter parsing | 4 (evolve x2 + discover + middleware) | 1 | `skills/metadata.rs::parse_frontmatter()` |
| Directory scanning | 4 (add + promote + discover + load) | 1 | `skills/loader.rs::scan_skill_dirs()` |
| Metadata read/write | 4 (evolve x3 + load) | 2 fns | `skills/metadata.rs::read/write_metadata()` |

---

## 6. File Size Targets (informational, not hard constraints)

| File | Before | After (est.) |
|------|--------|-------------|
| board.rs | 1095 | ~200 (struct + init + tables) |
| work_items.rs | (new) | ~300 |
| stamps.rs | (new) | ~250 |
| rigs.rs | (new) | ~150 |
| evolve.rs (skills) | 927 | 0 (split into 5 files, each <200) |
| evolver.rs (binary) | 457 | ~300 |
| api.rs | 922 | ~800 (coupling fix, not a split target) |
| load.rs | 820 | 0 (split into loader + lifecycle + catalog) |
| main.rs | 705 | ~100 |

---

## 7. Execution Order

1. Create `opengoose-skills` crate, move types + pure functions first (metadata, parser, validator, prompts)
2. Move file I/O (writer, loader, manage/*)
3. Update binary crate imports, verify `cargo test`
4. Split Board into modules (work_items, rigs, stamps)
5. Remove Board.db() from public API, add query methods
6. Fix web/api.rs coupling
7. Split main.rs (cli, runtime, commands/)
8. Split evolver.rs process_stamp()
9. Unify Agent creation in runtime.rs
10. Extract test_utils, deduplicate frontmatter/scan/metadata patterns
11. Final: `cargo test` + `cargo clippy` + manual TUI/Web/Worker verification

---

## Out of Scope (Plan C candidates)

- `opengoose-core` crate for shared types/errors
- Moving Evolver into rig crate
- api.rs internal split
- Performance benchmarks
