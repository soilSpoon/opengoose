# FP Quality Sweep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve code quality across 4 Rust crates via file decomposition, FP patterns, error handling, and test coverage.

**Architecture:** Risk-First Hybrid — error handling sweep first (per spec Phase 1), then decompose the largest files (evolver.rs 1,744L → api.rs 1,485L → ...), applying FP patterns and test improvements in each pass. Error types emerge from unwrap removal, not upfront design.

**Tech Stack:** Rust, anyhow/thiserror, tokio, sea-orm, axum, ratatui

**Spec:** `docs/superpowers/specs/2026-03-24-fp-quality-sweep-design.md`

**PR Strategy:**
- PR 1: Error handling sweep (Task 1)
- PR 2: High-risk decomposition — evolver, api, event (Tasks 2–4)
- PR 3: TUI + main + board decomposition (Tasks 5–7)
- PR 4: Rig crate decomposition (Tasks 8–10)
- PR 5: Skills crate decomposition (Tasks 11–13)
- PR 6: Test quality + coverage (Tasks 14–15)
- PR 7: Final cleanup (Task 16)

---

## File Structure

### New modules from decomposition

```
crates/opengoose/src/
├── evolver/
│   ├── mod.rs          ← re-exports, AgentCaller trait
│   ├── loop_driver.rs  ← run() main loop + lazy init
│   ├── pipeline.rs     ← prepare_context + execute_action + process_stamp
│   └── sweep.rs        ← run_sweep() offline re-evaluation
├── skills/
│   ├── evolve.rs       ← re-exports from opengoose-skills (thin wrapper, keep as-is if <500L)
│   └── load.rs         ← re-exports from opengoose-skills (thin wrapper, keep as-is if <500L)
├── web/
│   ├── api/
│   │   ├── mod.rs      ← router(), shared state
│   │   ├── board.rs    ← board_list, board_get, board_create, board_claim
│   │   ├── rigs.rs     ← rigs_list, rig_detail
│   │   └── skills.rs   ← SkillContext, skills_list, skill_detail, skill_promote, skill_delete
│   └── ...
├── tui/
│   ├── event/
│   │   ├── mod.rs      ← run_tui() main loop
│   │   ├── keys.rs     ← handle_key, handle_chat_key, handle_logs_key
│   │   ├── commands.rs ← handle_input, handle_task
│   │   └── rigs.rs     ← spawn_operator_reply, load_rigs
│   ├── app/
│   │   ├── mod.rs      ← App struct + delegation methods
│   │   ├── chat.rs     ← ChatState
│   │   ├── board.rs    ← BoardState
│   │   └── logs.rs     ← LogState
│   └── ui/
│       ├── mod.rs      ← render() top-level
│       ├── board.rs    ← render_board, render_rigs
│       ├── chat.rs     ← render_chat, chat_line_to_lines, render_input
│       └── logs.rs     ← render_logs, format_log_entry
├── cli.rs              ← CLI arg parsing (from main.rs)
├── runtime.rs          ← init_runtime() (from main.rs)
└── headless.rs         ← run_headless() (from main.rs)

crates/opengoose-board/src/
├── work_items/
│   ├── mod.rs          ← re-exports
│   ├── transitions.rs  ← post, claim, submit, unclaim, mark_stuck, retry, abandon
│   ├── queries.rs      ← get, list, ready, claimed_by, completed_by_rig
│   └── helpers.rs      ← transition(), sync_item, get_or_err, find_model, blocked_item_ids, compact
├── store/
│   ├── mod.rs          ← CowStore struct, CoW mutations, branch, discard, read access
│   ├── merge.rs        ← merge(), resolve_merge_item()
│   └── persist.rs      ← persist(), restore(), compute_root_hash(), append_commit()

crates/opengoose-rig/src/
├── rig/
│   ├── mod.rs          ← Rig<M> core, cancel, accessors
│   ├── operator.rs     ← Operator impl (chat, chat_streaming)
│   └── worker.rs       ← Worker impl (run, claim, execute, retry)
├── conversation_log/
│   ├── mod.rs          ← re-exports
│   ├── paths.rs        ← opengoose_home_dir, log_dir, log_path
│   ├── io.rs           ← append_entry, read_log, read_log_contents
│   └── retention.rs    ← list_logs, clean_older_than, clean_over_capacity
└── mcp_tools/
    ├── mod.rs          ← BoardClient struct, McpClientTrait impl
    ├── handlers.rs     ← handle_read_board, handle_claim_next, handle_submit, handle_create_task
    └── schema.rs       ← tools(), tool_def()

crates/opengoose-skills/src/
├── manage/
│   ├── discover/
│   │   ├── mod.rs      ← discover_skills() entry point
│   │   ├── scan.rs     ← scan_dir() recursive traversal
│   │   └── parse.rs    ← parse_skill_md, extract_frontmatter, yaml/json conversion
│   └── ...
├── evolution/
│   ├── writer/
│   │   ├── mod.rs      ← write_skill_to_rig_scope, update_existing_skill
│   │   ├── refine.rs   ← refine_skill
│   │   └── effectiveness.rs ← update_effectiveness_versioned, extract_name_from_content
│   └── ...
├── evolve.rs           ← evolution logic vs file creation vs validation (689L)
└── load.rs             ← filesystem traversal vs parsing vs caching (669L)
```

---

## Task 1: Error handling sweep — unwrap → Result propagation

**Files:** All production code across 4 crates

**Context:** Per spec Phase 1, error infrastructure goes first. Analysis found most production unwrap() calls are safe (with fallbacks), but this sweep establishes consistent patterns before decomposition begins. Current state: 140 tests passing.

- [ ] **Step 1: Run workspace-wide unwrap audit**

```bash
cargo clippy --workspace -- -W clippy::unwrap_used 2>&1 | head -100
```

Review output. Categorize each unwrap:
- **Safe (keep)**: `.unwrap_or()`, `.unwrap_or_default()`, `.unwrap_or_else()`
- **Convert to ?**: Where the caller returns Result
- **Convert to expect**: Where panic is intentional invariant
- **Need domain error**: Where caller should match and recover

- [ ] **Step 2: Apply conversions per crate**

Work through each crate bottom-up (board → rig → skills → opengoose). For each production unwrap:
- If caller returns `Result` → use `?` with `.context("msg")`
- If invariant that should never fail → use `.expect("reason")`
- If caller needs recovery → note for potential domain error type

- [ ] **Step 3: Identify domain error candidates**

From Step 2, list any error paths where callers actually match and recover. If 3+ cases emerge in a crate, create a domain error enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RigError {
    // only variants discovered from actual recovery patterns
}
```

- [ ] **Step 4: Verify and commit**

```bash
cargo test --workspace 2>&1 | tail -20
cargo clippy --workspace 2>&1 | tail -20
git commit -m "refactor: error handling sweep — unwrap to Result propagation"
```

---

## Task 2: Decompose evolver.rs (1,744L → 4 modules)

**Files:**
- Modify: `crates/opengoose/src/evolver.rs` → split into directory module
- Create: `crates/opengoose/src/evolver/mod.rs`
- Create: `crates/opengoose/src/evolver/loop_driver.rs`
- Create: `crates/opengoose/src/evolver/pipeline.rs`
- Create: `crates/opengoose/src/evolver/sweep.rs`

**Context:** evolver.rs has 4 natural boundaries:
1. Main loop + lazy init (fn `run`, lines 77–160)
2. Stamp processing pipeline (fn `prepare_context` + `execute_action` + `process_stamp`, lines 177–421)
3. Sweep logic (fn `run_sweep`, lines 424–517)
4. AgentCaller trait (lines 28–59)

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read the `#[cfg(test)]` module at the end of evolver.rs. Confirm tests cover: main loop behavior, stamp processing, and sweep logic. If any boundary has zero tests, add a minimal behavior-capture test before proceeding.

- [ ] **Step 2: Read evolver.rs and verify responsibility boundaries**

Read the full file. Confirm the 4 boundaries match what's described above. Note all `pub` vs `pub(crate)` vs private visibility and cross-references between functions.

- [ ] **Step 3: Create evolver/mod.rs with AgentCaller trait + re-exports**

Move the `AgentCaller` trait and `RealAgentCaller` impl to `mod.rs`. Add `mod loop_driver; mod pipeline; mod sweep;` declarations. Re-export `run` and any other public items so external callers don't break.

- [ ] **Step 4: Create evolver/loop_driver.rs**

Move fn `run()` (main loop, lazy init, stamp_notify + fallback sweep, idle-time sweep trigger). This function calls into `pipeline::process_stamp()` and `sweep::run_sweep()`, so import those.

- [ ] **Step 5: Create evolver/pipeline.rs**

Move `update_effectiveness()`, `prepare_context()`, `execute_action()`, `process_stamp()`. These form a linear pipeline: process_stamp orchestrates the other three.

- [ ] **Step 6: Create evolver/sweep.rs**

Move `run_sweep()` — offline re-evaluation of dormant/archived skills.

- [ ] **Step 7: Apply FP improvements in each new module**

- `pipeline.rs`: Extract pure validation/parsing from `execute_action()` into standalone functions. The response parsing (Create/Update/Skip decision) is pure logic — separate from the agent call + file write side effects.
- `loop_driver.rs`: The `agent.as_ref().unwrap()` on line 153 is safe (guarded by is_none check), but convert to `agent.as_ref().expect("lazy init guarantees Some")` for clarity.
- `sweep.rs`: The decision-matching loop (for decision in &decisions) — extract the match arms into a pure `fn apply_decision()` function that's independently testable.

- [ ] **Step 8: Move tests to appropriate modules**

Distribute existing tests from the `#[cfg(test)]` module to the new submodules they test. Each submodule gets its own `#[cfg(test)] mod tests`.

- [ ] **Step 9: Verify compilation and run tests**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: All tests pass, zero warnings about unused imports.

- [ ] **Step 10: Commit**

```bash
git add crates/opengoose/src/evolver/ crates/opengoose/src/evolver.rs
git commit -m "refactor(opengoose): decompose evolver.rs into 4 modules"
```

---

## Task 3: Decompose web/api.rs (1,485L → 4 modules)

**Files:**
- Modify: `crates/opengoose/src/web/api.rs` → split into directory module
- Create: `crates/opengoose/src/web/api/mod.rs`
- Create: `crates/opengoose/src/web/api/board.rs`
- Create: `crates/opengoose/src/web/api/rigs.rs`
- Create: `crates/opengoose/src/web/api/skills.rs`

**Context:** api.rs has 3 resource groups:
1. Board handlers (board_list, board_get, board_create, board_claim) — lines 51–169
2. Rigs handlers (rigs_list, rig_detail) — lines 75–227
3. Skills handlers (SkillContext, skills_list, skill_detail, skill_promote, skill_delete) — lines 265–437

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read the test module. Confirm tests cover each resource group. Add behavior-capture tests for any untested handler before splitting.

- [ ] **Step 2: Read api.rs and map all handler functions + shared state**

Identify the AppState struct, router setup, and which handlers share which state. Note the `default_rig()` and `skill_dirs()` helpers.

- [ ] **Step 3: Create api/mod.rs with router + shared state + helpers**

Move router definition, AppState, `default_rig()`, `skill_dirs()` to mod.rs. Add submodule declarations.

- [ ] **Step 4: Create api/board.rs**

Move `board_list`, `board_get`, `board_create`, `board_claim`. These all take `State<AppState>` and operate on `Board`.

- [ ] **Step 5: Create api/rigs.rs**

Move `rigs_list`, `rig_detail`. These query board + stamps for rig performance data.

- [ ] **Step 6: Create api/skills.rs**

Move `SkillContext` struct + impl, `skills_list`, `skill_detail`, `skill_promote`, `skill_delete`. The SkillContext collects skills from 3 scopes (global, project, rig).

- [ ] **Step 7: Apply FP improvements**

- `board.rs`: The `board_create` validation (title length, description length, priority mapping) — extract pure `fn validate_create_request(req: &CreateItem) -> Result<()>` for unit testing.
- `skills.rs`: The `SkillContext::collect_all_skills()` nested for loop (lines 309–317) — refactor to `flat_map` if clearer, or extract as named helper.

- [ ] **Step 8: Move tests to submodules**

Each handler module gets its own `#[cfg(test)] mod tests`.

- [ ] **Step 9: Verify and commit**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: All tests pass.

```bash
git add crates/opengoose/src/web/api/ crates/opengoose/src/web/api.rs
git commit -m "refactor(opengoose): decompose web/api.rs into board/rigs/skills modules"
```

---

## Task 4: Decompose tui/event.rs (1,261L → 4 modules)

**Files:**
- Modify: `crates/opengoose/src/tui/event.rs` → split into directory module
- Create: `crates/opengoose/src/tui/event/mod.rs`
- Create: `crates/opengoose/src/tui/event/keys.rs`
- Create: `crates/opengoose/src/tui/event/commands.rs`
- Create: `crates/opengoose/src/tui/event/rigs.rs`

**Context:** event.rs has 4 boundaries:
1. Main loop (`run_tui`, lines 31–122)
2. Key dispatch (`handle_key`, `handle_chat_key`, `handle_logs_key`, lines 125–272)
3. Command parsing (`handle_input`, `handle_task`, lines 275–350)
4. Worker integration (`spawn_operator_reply`, `load_rigs`, lines 353–409)

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read the test module. Confirm tests cover key dispatch and command parsing. Add behavior-capture tests for any untested boundary.

- [ ] **Step 2: Read event.rs and map all function signatures + dependencies**

- [ ] **Step 3: Create event/mod.rs with run_tui()**

The main loop stays in mod.rs since it's the entry point. It calls into keys, commands, and rigs modules.

- [ ] **Step 4: Create event/keys.rs**

Move `handle_key`, `handle_chat_key`, `handle_logs_key`. Key dispatch is self-contained — routes to commands or updates app state.

- [ ] **Step 5: Create event/commands.rs**

Move `handle_input`, `handle_task`. Command parsing (/board, /task, /quit).

- [ ] **Step 6: Create event/rigs.rs**

Move `spawn_operator_reply`, `load_rigs`. Worker/rig integration.

- [ ] **Step 7: Apply FP improvements**

- `keys.rs`: The 2 production unwrap calls (`.chars().next().unwrap()` at lines 239, 248) are safe (guarded by byte_pos check), but add `.expect("byte_pos < len guarantees char exists")`.
- `commands.rs`: Extract command parsing into a pure `fn parse_command(input: &str) -> Command` enum that returns `Command::Board | Command::Task(title) | Command::Quit | Command::Chat(text)`. Then `handle_input` just matches on this enum. The parsing logic is testable without async/board dependencies.

- [ ] **Step 8: Move tests, verify, commit**

Run: `cargo test --workspace 2>&1 | tail -20`

```bash
git add crates/opengoose/src/tui/event/ crates/opengoose/src/tui/event.rs
git commit -m "refactor(opengoose): decompose tui/event.rs into keys/commands/rigs modules"
```

---

## Task 5: Decompose tui/app.rs (510L) + tui/ui.rs (542L)

**Files:**
- Modify: `crates/opengoose/src/tui/app.rs` → split into directory module
- Create: `crates/opengoose/src/tui/app/mod.rs`
- Create: `crates/opengoose/src/tui/app/chat.rs`
- Create: `crates/opengoose/src/tui/app/board.rs`
- Create: `crates/opengoose/src/tui/app/logs.rs`
- Modify: `crates/opengoose/src/tui/ui.rs` → split into directory module
- Create: `crates/opengoose/src/tui/ui/mod.rs`
- Create: `crates/opengoose/src/tui/ui/board.rs`
- Create: `crates/opengoose/src/tui/ui/chat.rs`
- Create: `crates/opengoose/src/tui/ui/logs.rs`

**Context:**
- app.rs: ChatState, BoardState, LogState, App (aggregator), RigInfo/RigStatus
- ui.rs: render functions grouped by tab (board, chat, logs)

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read test modules in both files. app.rs has 16 tests, ui.rs has tests for render functions. Confirm coverage for all state types and render functions.

- [ ] **Step 2: Read both files, map state types ↔ render functions**

- [ ] **Step 3: Split app.rs — ChatState → chat.rs, BoardState → board.rs, LogState → logs.rs**

App struct stays in mod.rs with delegation methods. Each state module owns its struct + methods.

- [ ] **Step 4: Split ui.rs — render functions by tab**

- `ui/mod.rs`: `render()`, `render_tab_bar()`, `render_current_tab()`
- `ui/board.rs`: `render_board()`, `render_rigs()`
- `ui/chat.rs`: `render_chat()`, `chat_line_to_lines()`, `render_input()`
- `ui/logs.rs`: `render_logs()`, `format_log_entry()`

- [ ] **Step 5: FP improvements in app state modules**

- `board.rs`: `active_items()` uses `let mut items` + `sort_by_key` + `reverse`. Refactor to use `.sorted_by_key()` (from itertools) or keep sort but remove the separate reverse by using `.sort_by(|a, b| b.cmp(a))`.
- `ui/board.rs`: `render_board()` and `render_rigs()` build `Vec<ListItem>` with `let mut items`. Convert to iterator chain: `items.iter().map(|item| ListItem::new(...)).collect()`.

- [ ] **Step 6: Move tests, verify, commit**

```bash
git add crates/opengoose/src/tui/app/ crates/opengoose/src/tui/ui/
git commit -m "refactor(opengoose): decompose tui app + ui into tab-based modules"
```

---

## Task 6: Decompose main.rs (829L → cli + runtime + headless)

**Files:**
- Modify: `crates/opengoose/src/main.rs` (keep minimal entry point)
- Create: `crates/opengoose/src/cli.rs`
- Create: `crates/opengoose/src/runtime.rs`
- Create: `crates/opengoose/src/headless.rs`

**Context:** main.rs has 3 boundaries:
1. CLI parsing + logging setup (fn `main`, lines 48–124)
2. Runtime init (fn `init_runtime`, lines 133–159)
3. Headless mode (fn `run_headless`, lines 163–210)

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read test module. main.rs has tests — verify they cover runtime init and headless mode. Add behavior-capture tests if missing.

- [ ] **Step 2: Read main.rs, map dependencies between sections**

- [ ] **Step 3: Create cli.rs**

Move CLI arg parsing (clap structs, subcommand enum) and logging setup logic. Keep `main()` in main.rs but it calls `cli::parse()` and routes.

- [ ] **Step 4: Create runtime.rs**

Move `init_runtime()` — Board connect, web server, Evolver, Worker spawning.

- [ ] **Step 5: Create headless.rs**

Move `run_headless()` — task posting, completion waiting, timeout handling.

- [ ] **Step 6: Slim down main.rs**

main.rs should be ~30-50 lines: `fn main()` that parses CLI, sets up logging, and routes to the right module.

- [ ] **Step 7: FP improvements**

- `cli.rs`: Extract logging setup into `fn setup_logging(mode: RunMode) -> Result<()>` — pure configuration, no side effects until returned.
- `headless.rs`: The `run_headless` tokio::select loop is already well-structured. No changes needed.

- [ ] **Step 8: Verify and commit**

```bash
git add crates/opengoose/src/main.rs crates/opengoose/src/cli.rs crates/opengoose/src/runtime.rs crates/opengoose/src/headless.rs
git commit -m "refactor(opengoose): decompose main.rs into cli/runtime/headless modules"
```

---

## Task 7: Decompose work_items.rs (926L → 3 modules)

**Files:**
- Modify: `crates/opengoose-board/src/work_items.rs` → split into directory module
- Create: `crates/opengoose-board/src/work_items/mod.rs`
- Create: `crates/opengoose-board/src/work_items/transitions.rs`
- Create: `crates/opengoose-board/src/work_items/queries.rs`
- Create: `crates/opengoose-board/src/work_items/helpers.rs`

**Context:** work_items.rs has zero production unwraps. Boundaries:
1. State transitions: post, claim, submit, unclaim, mark_stuck, retry, abandon (lines 12–145)
2. Queries: get, list, ready, claimed_by, completed_by_rig (lines 147–206)
3. Helpers: transition(), sync_item, get_or_err, find_model, blocked_item_ids, compact (lines 207–364)

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read test module. work_items.rs has comprehensive tests — verify coverage for transitions, queries, and helpers.

- [ ] **Step 2: Read work_items.rs, confirm boundary accuracy**

- [ ] **Step 3: Create work_items/mod.rs with re-exports**

Board impl blocks are split across files. Use `impl Board` in each submodule (Rust allows multiple impl blocks for the same type across modules within the same crate).

- [ ] **Step 4: Create transitions.rs**

Move `post`, `claim`, `submit`, `unclaim`, `mark_stuck`, `retry`, `abandon`. All follow the pattern: validate → transition() → notify.

- [ ] **Step 5: Create queries.rs**

Move `get`, `list`, `ready`, `claimed_by`, `completed_by_rig`. Pure read-only operations.

- [ ] **Step 6: Create helpers.rs**

Move `transition()`, `sync_item()`, `get_or_err()`, `find_model()`, `blocked_item_ids()`, `compact()`. Internal helpers used by transitions.

- [ ] **Step 7: FP improvements**

- `queries.rs`: `ready()` and `claimed_by()` both use `let mut items` + sort. Check if sort can be done inline or if the pattern is already minimal.
- `helpers.rs`: `compact()` has a for loop with per-item async txn — this cannot be chained. Leave as-is.

- [ ] **Step 8: Move tests, verify, commit**

```bash
git add crates/opengoose-board/src/work_items/
git commit -m "refactor(board): decompose work_items.rs into transitions/queries/helpers"
```

---

## Task 8: Decompose rig.rs (590L → 3 modules)

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs` → split into directory module
- Create: `crates/opengoose-rig/src/rig/mod.rs`
- Create: `crates/opengoose-rig/src/rig/operator.rs`
- Create: `crates/opengoose-rig/src/rig/worker.rs`

**Context:** rig.rs has 3 impl blocks:
1. `impl<M: WorkMode> Rig<M>` — generic core (new, process, accessors, cancel)
2. `impl Operator` — chat, chat_streaming
3. `impl Worker` — run, try_claim_and_execute, process_claimed_item, retry, etc.

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read test module. Confirm tests cover operator chat and worker pull loop behavior.

- [ ] **Step 2: Read rig.rs, map cross-references between impl blocks**

- [ ] **Step 3: Create rig/mod.rs with Rig<M> core**

Move `impl<M: WorkMode> Rig<M>` (new, process, agent, board, cancel, cancel_token) + `extract_text_content()` standalone function. Add `mod operator; mod worker;` and type aliases.

- [ ] **Step 4: Create rig/operator.rs**

Move `impl Operator` (without_board, chat, chat_streaming).

- [ ] **Step 5: Create rig/worker.rs**

Move `impl Worker` (run, try_claim_and_execute, try_claim_first, process_claimed_item, acquire_worktree, resolve_session, execute_with_retry, find_session_by_name).

- [ ] **Step 6: FP improvements**

- `worker.rs`: `try_claim_first()` has a for loop over candidates (lines 241–247). Evaluate whether `.iter().find_map()` is clearer — note this is async, so a manual loop may be necessary. Prioritize clarity.
- `worker.rs`: `.expect("Worker must have a board")` at line 216 — replace with `let board = self.board().ok_or_else(|| anyhow!("Worker must have a board"))?;` to propagate instead of panic.

- [ ] **Step 7: Move tests, verify, commit**

```bash
git add crates/opengoose-rig/src/rig/
git commit -m "refactor(rig): decompose rig.rs into core/operator/worker modules"
```

---

## Task 9: Decompose conversation_log.rs (486L → 3 modules)

**Files:**
- Modify: `crates/opengoose-rig/src/conversation_log.rs` → split into directory module
- Create: `crates/opengoose-rig/src/conversation_log/mod.rs`
- Create: `crates/opengoose-rig/src/conversation_log/paths.rs`
- Create: `crates/opengoose-rig/src/conversation_log/io.rs`
- Create: `crates/opengoose-rig/src/conversation_log/retention.rs`

**Context:** Functions group naturally:
1. Paths: opengoose_home_dir, log_dir, log_path
2. IO: append_entry, read_log, read_log_contents + LogEntry custom Deserialize impl
3. Retention: list_logs, clean_older_than, clean_over_capacity

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read test module. conversation_log.rs has 18 tests — verify coverage across all 3 boundaries.

- [ ] **Step 2: Read file, confirm boundaries, split into 3 modules**

Move functions to their modules. LogEntry struct and its Deserialize impl go to `io.rs` (or `mod.rs` if shared).

- [ ] **Step 3: FP improvements**

- `retention.rs`: `clean_older_than()` (line 132) — the for loop with `removed += 1` can become:
  ```rust
  let removed = logs.iter()
      .filter(|log| log.modified < cutoff)
      .filter(|log| std::fs::remove_file(&log.path).is_ok())
      .count();
  ```
- `clean_over_capacity()` has early break logic — leave as for loop (try_fold would be less clear).

- [ ] **Step 4: Move tests, verify, commit**

```bash
git add crates/opengoose-rig/src/conversation_log/
git commit -m "refactor(rig): decompose conversation_log.rs into paths/io/retention"
```

---

## Task 10: Decompose mcp_tools.rs (528L → 3 modules)

**Files:**
- Modify: `crates/opengoose-rig/src/mcp_tools.rs` → split into directory module
- Create: `crates/opengoose-rig/src/mcp_tools/mod.rs`
- Create: `crates/opengoose-rig/src/mcp_tools/handlers.rs`
- Create: `crates/opengoose-rig/src/mcp_tools/schema.rs`

**Context:**
1. BoardClient struct + McpClientTrait impl → mod.rs
2. Handler functions (handle_read_board, handle_claim_next, handle_submit, handle_create_task) → handlers.rs
3. Tool schema definitions (tools(), tool_def()) → schema.rs

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read test module. Confirm tests cover tool listing, board operations, and error cases.

- [ ] **Step 2: Read file, verify structure, split into 3 modules**

- [ ] **Step 3: FP improvements**

- `mod.rs`: The `let mut info = InitializeResult::default()` + field assignment pattern — consider builder pattern or struct literal if clearer.
- `handlers.rs`: handler functions already use Result chains well. Minimal changes expected.

- [ ] **Step 4: Move tests, verify, commit**

```bash
git add crates/opengoose-rig/src/mcp_tools/
git commit -m "refactor(rig): decompose mcp_tools.rs into handlers/schema modules"
```

---

## Task 11: Decompose store.rs (474L → 3 modules)

**Files:**
- Modify: `crates/opengoose-board/src/store.rs` → split into directory module
- Create: `crates/opengoose-board/src/store/mod.rs`
- Create: `crates/opengoose-board/src/store/merge.rs`
- Create: `crates/opengoose-board/src/store/persist.rs`

**Context:** store.rs has clear boundaries:
1. CowStore struct, new(), from_items(), CoW mutations (insert/update/remove), branch, discard, read access → mod.rs
2. merge(), resolve_merge_item() → merge.rs
3. persist(), restore(), compute_root_hash(), append_commit() → persist.rs

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read test module. store.rs has 10 tests — verify coverage for branching, merging, and persistence.

- [ ] **Step 2: Read file, confirm boundaries, split into 3 modules**

- [ ] **Step 3: FP improvements**

- `merge.rs`: `resolve_merge_item()` is already a pure function — no changes needed.
- `persist.rs`: The for loops in persist/restore use async operations — leave as-is.

- [ ] **Step 4: Move tests, verify, commit**

```bash
git add crates/opengoose-board/src/store/
git commit -m "refactor(board): decompose store.rs into core/merge/persist"
```

---

## Task 12: Decompose discover.rs (524L → 3 modules)

**Files:**
- Modify: `crates/opengoose-skills/src/manage/discover.rs` → split into directory module
- Create: `crates/opengoose-skills/src/manage/discover/mod.rs`
- Create: `crates/opengoose-skills/src/manage/discover/scan.rs`
- Create: `crates/opengoose-skills/src/manage/discover/parse.rs`

**Context:** discover.rs has 3 boundaries:
1. Entry point: discover_skills() → mod.rs
2. Directory traversal: scan_dir() recursive depth-limited scan → scan.rs
3. Parsing: parse_skill_md, extract_frontmatter, yaml_to_json, serde_yaml_or_fallback → parse.rs

- [ ] **Step 1: Verify existing test coverage; add safety-net tests if gaps exist**

Read test module. discover.rs has 18 tests — verify coverage for discovery, dedup, parsing, and edge cases.

- [ ] **Step 2: Read file, confirm boundaries, split into 3 modules**

- [ ] **Step 3: FP improvements**

- `parse.rs`: `yaml_to_json()` has a stateful for loop with `in_metadata` flag. Evaluate if this could be a fold, but likely clearer as-is.
- `scan.rs`: `scan_dir()` uses recursion within a for loop — leave as-is (iterator chaining would obscure the recursion).

- [ ] **Step 4: Move tests, verify, commit**

```bash
git add crates/opengoose-skills/src/manage/discover/
git commit -m "refactor(skills): decompose discover.rs into scan/parse modules"
```

---

## Task 13: Decompose writer.rs (438L) + assess evolve.rs (689L) + load.rs (669L)

**Files:**
- Modify: `crates/opengoose-skills/src/evolution/writer.rs` → split into directory module
- Create: `crates/opengoose-skills/src/evolution/writer/mod.rs`
- Create: `crates/opengoose-skills/src/evolution/writer/refine.rs`
- Create: `crates/opengoose-skills/src/evolution/writer/effectiveness.rs`
- Assess: `crates/opengoose-skills/src/evolve.rs` (689L)
- Assess: `crates/opengoose-skills/src/load.rs` (669L)

**Context:**
- writer.rs boundaries: write/update (mod.rs), refine (refine.rs), effectiveness tracking (effectiveness.rs)
- evolve.rs (689L) and load.rs (669L) are listed in the spec. Analysis revealed they are thin wrapper/re-export layers (~20 lines production code + large test modules). Read them to determine if decomposition provides value or if the size comes from tests.

- [ ] **Step 1: Verify existing test coverage for writer.rs**

Read test module. writer.rs has 6 tests — verify coverage for write, update, refine, and effectiveness.

- [ ] **Step 2: Split writer.rs into 3 modules**

- `writer/mod.rs`: write_skill_to_rig_scope, update_existing_skill
- `writer/refine.rs`: refine_skill
- `writer/effectiveness.rs`: update_effectiveness_versioned, extract_name_from_content

- [ ] **Step 3: Assess evolve.rs and load.rs**

Read both files. If they are primarily re-export wrappers with large test modules (as analysis suggests):
- If production code is <100 lines and tests are the bulk → keep as single files, just apply FP/test quality improvements
- If there are meaningful responsibility boundaries in the production code → split accordingly

Document the decision in the commit message.

- [ ] **Step 4: FP improvements**

- `writer/effectiveness.rs`: `update_effectiveness_versioned()` reads file → modifies → writes. Extract the pure modification logic into a separate function.
- For evolve.rs/load.rs: apply any FP improvements identified during assessment.

- [ ] **Step 5: Move tests, verify, commit**

```bash
git commit -m "refactor(skills): decompose writer.rs into write/refine/effectiveness"
git commit -m "refactor(skills): assess and improve evolve.rs + load.rs"
```

---

## Task 14: Test quality improvements

**Files:** All `#[cfg(test)]` modules across 4 crates

**Context:** 140 tests exist across the workspace. Improvements:
1. Test unwrap() → `.expect("description")` for better failure messages
2. Extract repeated setup into test helpers
3. Rename tests to describe behavior

- [ ] **Step 1: Audit test patterns per crate**

For each crate, read the test modules and identify:
- Repeated setup code (Board init, tempdir creation, fixture building)
- Unclear test names (`test_1`, `test_basic`)
- Bare `.unwrap()` that could be `.expect("what we're testing")`

- [ ] **Step 2: Create test helpers where 3+ tests share setup**

For each crate, if 3+ tests share identical setup, extract to a helper:

```rust
#[cfg(test)]
mod testutil {
    pub fn test_board() -> Board { ... }
    pub fn test_work_item() -> WorkItem { ... }
}
```

Place in each crate's `src/testutil.rs` (or within the test module if crate-internal only).

- [ ] **Step 3: Rename tests to behavior-describing names**

Pattern: `{action}_{condition}_{expected_result}`
- `test_claim` → `claim_open_item_succeeds`
- `test_claim_error` → `claim_already_claimed_returns_error`

- [ ] **Step 4: Convert test unwrap() to expect()**

```rust
// Before
board.post(item).await.unwrap();

// After
board.post(item).await.expect("posting new item should succeed");
```

- [ ] **Step 5: Verify and commit**

```bash
cargo test --workspace 2>&1 | tail -20
git commit -m "test: improve test quality — helpers, naming, expect messages"
```

---

## Task 15: Test coverage gap analysis + new tests

**Files:** Modules identified as under-tested

**Context:** After decomposition, each new module should have tests. Focus on:
1. Pure functions extracted during FP refactoring (highest value — easy to test)
2. Error paths (currently most tests are happy-path)
3. Edge cases in complex logic (merge, sweep, state transitions)

- [ ] **Step 1: Run coverage analysis**

```bash
cargo install cargo-llvm-cov  # if not installed
cargo llvm-cov --workspace --lcov --output-path lcov.info 2>&1 | tail -30
```

Or if cargo-llvm-cov isn't available, manually identify untested functions by reading each module and checking for corresponding test coverage.

- [ ] **Step 2: Add tests for extracted pure functions**

Every pure function created during FP refactoring (Tasks 2-13) should have at least:
- One happy-path test
- One error/edge-case test

Example targets:
- `evolver/pipeline.rs`: response parsing logic
- `evolver/sweep.rs`: `apply_decision()` pure function
- `tui/event/commands.rs`: `parse_command()` enum
- `web/api/board.rs`: `validate_create_request()`

- [ ] **Step 3: Add error path tests**

For state transitions (work_items/transitions.rs):
- claim an already-claimed item → error
- submit by wrong rig → error
- transition from invalid state → error

For conversation_log/retention.rs:
- clean with no logs → returns 0
- clean with all logs older than cutoff → removes all

- [ ] **Step 4: Verify all tests pass**

```bash
cargo test --workspace
```

Expected: Previous 140 tests + new tests all pass.

- [ ] **Step 5: Commit**

```bash
git commit -m "test: add coverage for extracted pure functions and error paths"
```

---

## Task 16: Final verification + cleanup

- [ ] **Step 1: Full build verification**

```bash
cargo build --workspace 2>&1
cargo test --workspace 2>&1
cargo clippy --workspace -- -D warnings 2>&1
```

All three must pass with zero errors and zero warnings.

- [ ] **Step 2: Check no dead code**

```bash
cargo build --workspace 2>&1 | grep "warning.*dead_code"
```

Remove any unused imports, functions, or types introduced during decomposition. Do NOT use `#[allow(dead_code)]`.

- [ ] **Step 3: Verify module re-exports**

Confirm that all public APIs are properly re-exported through mod.rs files. External callers (other crates, tests) should not need to change their import paths — or if they do, all usages have been updated.

- [ ] **Step 4: Final commit**

```bash
git commit -m "chore: final cleanup — dead code removal, import fixes"
```
