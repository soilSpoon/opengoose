# PR #340 Review Follow-up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address all unresolved CodeRabbit review comments from PR #340 — trait DI for evolver, bug fix, test isolation, and test quality improvements.

**Architecture:** 7 independent changes. The biggest is extracting `AgentCaller` trait in evolver.rs to replace `cfg!(test)` + env var mocking with proper dependency injection. All other changes are localized to their respective files.

**Tech Stack:** Rust, async-trait, tokio, tempfile

**Spec:** `docs/superpowers/specs/2026-03-21-pr340-review-followup-design.md`

**Naming convention:** Do NOT use `_` prefix/suffix for variable names. Use `guard` not `_guard`, `env` not `_env`, etc. Fix existing violations in files you touch.

---

## Task 1: EvolveMode session_id bug fix

**Files:**
- Modify: `crates/opengoose-rig/src/work_mode.rs:103-116` (EvolveMode impl)
- Test: same file, test module

- [ ] **Step 1: Write the failing test**

Add to the existing test module in `crates/opengoose-rig/src/work_mode.rs`:

```rust
#[test]
fn evolve_mode_uses_presupplied_session_id() {
    let mode = EvolveMode;
    let input = WorkInput::chat("x").with_session_id("pre-evolve".into());
    assert_eq!(mode.session_for(&input), "pre-evolve");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p opengoose-rig -- evolve_mode_uses_presupplied_session_id -v`
Expected: FAIL — returns `"evolve-<timestamp>"` instead of `"pre-evolve"`

- [ ] **Step 3: Fix EvolveMode::session_for**

In `crates/opengoose-rig/src/work_mode.rs`, change `EvolveMode::session_for` (lines 104-115) to check `session_id` first, matching the pattern in `TaskMode::session_for` (lines 82-97):

```rust
impl WorkMode for EvolveMode {
    fn session_for(&self, input: &WorkInput) -> String {
        if let Some(id) = &input.session_id {
            return id.clone();
        }
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

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p opengoose-rig -- work_mode -v`
Expected: ALL work_mode tests pass including new one

- [ ] **Step 5: Commit**

```
fix: EvolveMode::session_for honors pre-supplied session_id
```

---

## Task 2: AgentCaller trait DI for evolver.rs

**Files:**
- Modify: `crates/opengoose/src/evolver.rs` (production code + all 23 tests)

This is the largest task. It replaces `cfg!(test)` + `OPENGOOSE_TEST_CALL_AGENT` env var with a proper `AgentCaller` trait.

### Step 2a: Add AgentCaller trait and RealAgentCaller

- [ ] **Step 2a-0: Add async-trait dependency**

In `crates/opengoose/Cargo.toml`, add `async-trait` to `[dependencies]`:

```toml
async-trait = "0.1"
```

Note: `async-trait` is already a workspace dependency (used in opengoose-rig). Just add it to opengoose's Cargo.toml.

- [ ] **Step 2a-1: Add trait definition and real implementation**

At the top of `crates/opengoose/src/evolver.rs`, after the imports (after line 14), add:

```rust
use async_trait::async_trait;

#[async_trait]
pub(crate) trait AgentCaller: Send + Sync {
    async fn call(&self, prompt: &str, work_id: i64) -> anyhow::Result<String>;
}

struct RealAgentCaller<'a> {
    agent: &'a Agent,
}

#[async_trait]
impl AgentCaller for RealAgentCaller<'_> {
    async fn call(&self, prompt: &str, work_id: i64) -> anyhow::Result<String> {
        let message = Message::user().with_text(prompt);
        let session_config = SessionConfig {
            id: format!("evolve-{work_id}"),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        let stream = self.agent.reply(message, session_config, None).await?;
        tokio::pin!(stream);

        let mut response_text = String::new();
        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Message(msg)) => {
                    use goose::conversation::message::MessageContent;
                    for content in &msg.content {
                        if let MessageContent::Text(t) = content {
                            response_text.push_str(&t.text);
                        }
                    }
                }
                Err(e) => return Err(e),
                _ => {}
            }
        }

        Ok(response_text)
    }
}
```

- [ ] **Step 2a-2: Verify it compiles**

Run: `cargo check -p opengoose`
Expected: compiles (trait + impl are unused but valid)

### Step 2b: Change function signatures

- [ ] **Step 2b-1: Change `execute_action` signature**

In `crates/opengoose/src/evolver.rs`, change `execute_action` (line 215):

From:
```rust
async fn execute_action(
    base_dir: &Path,
    board: &Board,
    agent: &Agent,
    stamp: &opengoose_board::entity::stamp::Model,
    ctx: &StampContext,
    existing: &[load::LoadedSkill],
) -> anyhow::Result<()> {
```

To:
```rust
async fn execute_action(
    base_dir: &Path,
    board: &Board,
    caller: &dyn AgentCaller,
    stamp: &opengoose_board::entity::stamp::Model,
    ctx: &StampContext,
    existing: &[load::LoadedSkill],
) -> anyhow::Result<()> {
```

Then replace all `call_agent(agent,` calls inside `execute_action` with `caller.call(`:
- Line ~227: `call_agent(agent, &ctx.prompt, ctx.evolver_item_id)` → `caller.call(&ctx.prompt, ctx.evolver_item_id)`
- Line ~251: `call_agent(agent, &update_prompt, ctx.evolver_item_id)` → `caller.call(&update_prompt, ctx.evolver_item_id)`
- Line ~307: `call_agent(agent, &retry_prompt, ctx.evolver_item_id)` → `caller.call(&retry_prompt, ctx.evolver_item_id)`

- [ ] **Step 2b-2: Change `process_stamp` signature**

Change `process_stamp` (line 350):

From:
```rust
async fn process_stamp(
    board: &Board,
    agent: &Agent,
    stamp: &opengoose_board::entity::stamp::Model,
) -> anyhow::Result<()> {
```

To:
```rust
async fn process_stamp(
    board: &Board,
    caller: &dyn AgentCaller,
    stamp: &opengoose_board::entity::stamp::Model,
) -> anyhow::Result<()> {
```

Update the call to `execute_action` inside `process_stamp` (~line 361):
`execute_action(&base_dir, board, agent, stamp, &ctx, &existing)` → `execute_action(&base_dir, board, caller, stamp, &ctx, &existing)`

- [ ] **Step 2b-3: Change `run_sweep` signature**

Change `run_sweep` (line 377):

From:
```rust
async fn run_sweep(board: &Board, agent: &Agent) -> anyhow::Result<()> {
```

To:
```rust
async fn run_sweep(board: &Board, caller: &dyn AgentCaller) -> anyhow::Result<()> {
```

Update the call to `call_agent` inside `run_sweep` (~line 436):
`call_agent(agent, &prompt, 0)` → `caller.call(&prompt, 0)`

- [ ] **Step 2b-4: Update `run()` to use RealAgentCaller**

In the `run()` function:

Update the sweep branch (~line 68):
```rust
// Before:
if let Err(e) = run_sweep(&board, agent).await {

// After:
let caller = RealAgentCaller { agent };
if let Err(e) = run_sweep(&board, &caller).await {
```

Update the stamp processing loop (~line 106):
```rust
// Before:
if let Err(e) = process_stamp(&board, agent.as_ref().unwrap(), stamp).await {

// After:
let caller = RealAgentCaller { agent: agent.as_ref().unwrap() };
if let Err(e) = process_stamp(&board, &caller, stamp).await {
```

- [ ] **Step 2b-5: Delete old `call_agent` function**

Remove the entire `call_agent` function (lines 472-523). All its logic is now in `RealAgentCaller::call()` (production) and will be in `MockAgentCaller::call()` (test).

- [ ] **Step 2b-6: Verify production code compiles**

Run: `cargo check -p opengoose`
Expected: compiles with test errors (tests still reference old `call_agent` and `Agent::new()`)

### Step 2c: Add MockAgentCaller and migrate tests

- [ ] **Step 2c-1: Add MockAgentCaller to test module**

In the `#[cfg(test)]` module of `evolver.rs`, add at the top:

```rust
use super::AgentCaller;

struct MockAgentCaller {
    reply: String,
}

#[async_trait::async_trait]
impl AgentCaller for MockAgentCaller {
    async fn call(&self, prompt: &str, work_id: i64) -> anyhow::Result<String> {
        let raw = if prompt.contains("Previous output had format errors") {
            self.reply
                .split("||")
                .nth(1)
                .unwrap_or(&self.reply)
                .to_string()
        } else {
            self.reply
                .split("||")
                .next()
                .unwrap_or(&self.reply)
                .to_string()
        };
        if let Some(err_msg) = raw.strip_prefix("ERR:") {
            return Err(anyhow::anyhow!(err_msg.to_string()));
        }
        Ok(raw)
    }
}
```

- [ ] **Step 2c-2: Migrate all tests using OPENGOOSE_TEST_CALL_AGENT**

For each of the 20 tests that use `OPENGOOSE_TEST_CALL_AGENT`, apply this pattern:

**Before:**
```rust
let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("REPLY_VALUE"));
let agent = Agent::new();
// ... test using process_stamp(&board, &agent, &stamp) or run_sweep(&board, &agent) ...
restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);
```

**After:**
```rust
let caller = MockAgentCaller { reply: "REPLY_VALUE".into() };
// ... test using process_stamp(&board, &caller, &stamp) or run_sweep(&board, &caller) ...
```

Key changes per test:
- Remove `set_env_var("OPENGOOSE_TEST_CALL_AGENT", ...)` line
- Remove `Agent::new()` line
- Add `MockAgentCaller { reply: "..." }` line
- Change `&agent` to `&caller` in `process_stamp`/`run_sweep` calls
- Remove `restore_env_var("OPENGOOSE_TEST_CALL_AGENT", ...)` line
- If the test ONLY used env lock for OPENGOOSE_TEST_CALL_AGENT (not HOME), remove the `test_env_lock()` line

**Tests that still need `test_env_lock()` and HOME setup** (they use `set_env_var("HOME", ...)`):
- `process_stamp_creates_skill_on_valid_evolve_output`
- `process_stamp_retries_when_first_output_invalid`
- `process_stamp_marks_update_without_skill_file`
- `run_sweep_deletes_dormant_skill_from_decision`
- `run_sweep_keeps_dormant_skill_from_decision`
- `run_sweep_restores_dormant_skill_from_decision`
- `run_sweep_refines_dormant_skill_from_decision`
- `process_stamp_update_skill_found_update_response_not_create`
- `process_stamp_retry_succeeds_when_second_output_valid`
- `run_sweep_covers_effectiveness_branch_variants`
- `process_stamp_with_installed_and_no_metadata_learned_skills`
- `process_stamp_retry_succeeds_and_writes_skill`
- All `run_sweep_*_nonexistent_*` and `run_sweep_refine_invalid_*` tests

**Tests that become lock-free** (no HOME, only OPENGOOSE_TEST_CALL_AGENT):
- `process_stamp_skips_when_agent_returns_skip`
- `process_stamp_propagates_agent_error_without_submit`
- `call_agent_returns_err_for_err_prefix` → convert to direct `MockAgentCaller::call()` test
- `call_agent_uses_correct_split_based_on_prompt_content` → convert to direct `MockAgentCaller::call()` test

For the two direct `call_agent` tests, convert to test `MockAgentCaller` directly:

```rust
#[tokio::test]
async fn mock_agent_caller_returns_err_for_err_prefix() {
    let caller = MockAgentCaller { reply: "ERR:test error message".into() };
    let result = caller.call("normal prompt", 0).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("test error message"));
}

#[tokio::test]
async fn mock_agent_caller_uses_correct_split_based_on_prompt_content() {
    let caller = MockAgentCaller { reply: "first-part||second-part".into() };

    let normal = caller.call("normal prompt", 0).await.unwrap();
    assert_eq!(normal, "first-part");

    let retry = caller
        .call("some context\n\nPrevious output had format errors: missing name", 0)
        .await
        .unwrap();
    assert_eq!(retry, "second-part");
}
```

- [ ] **Step 2c-3: Fix naming — rename `_guard` to `guard` in all evolver tests**

In all test functions, change:
```rust
let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
```
to:
```rust
let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
```

- [ ] **Step 2c-4: Cleanup — remove unused helpers**

If `set_env_var` and `restore_env_var` are no longer called with `"OPENGOOSE_TEST_CALL_AGENT"` anywhere:
- Check if they're still used for `"HOME"` — if yes, keep them
- If no remaining callers, delete them

Remove `use goose::agents::Agent;` from test module if `Agent::new()` is no longer used anywhere in tests.

- [ ] **Step 2c-5: Run all evolver tests**

Run: `cargo test -p opengoose -- evolver -v`
Expected: ALL tests pass (same count as before)

- [ ] **Step 2c-6: Run full test suite**

Run: `cargo test -p opengoose`
Expected: all pass, no regressions

- [ ] **Step 2c-7: Commit**

```
refactor: replace cfg!(test) env var mock with AgentCaller trait DI in evolver
```

---

## Task 3: EnvGuard for with_clean_home()

**Files:**
- Modify: `crates/opengoose/src/skills/mod.rs` (test module, lines 104-143)

- [ ] **Step 1: Add EnvGuard struct to test module**

In `crates/opengoose/src/skills/mod.rs`, inside the `#[cfg(test)]` module, add:

```rust
struct EnvGuard {
    home: Option<std::ffi::OsString>,
    opengoose_home: Option<std::ffi::OsString>,
    xdg_state_home: Option<std::ffi::OsString>,
    cwd: std::path::PathBuf,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match &self.opengoose_home {
                Some(v) => std::env::set_var("OPENGOOSE_HOME", v),
                None => std::env::remove_var("OPENGOOSE_HOME"),
            }
            match &self.xdg_state_home {
                Some(v) => std::env::set_var("XDG_STATE_HOME", v),
                None => std::env::remove_var("XDG_STATE_HOME"),
            }
            let _ = std::env::set_current_dir(&self.cwd);
        }
    }
}
```

- [ ] **Step 2: Refactor with_clean_home() to use EnvGuard**

Replace the current `with_clean_home` implementation (lines 104-143) with:

```rust
async fn with_clean_home<F, Fut>(f: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempdir().unwrap();

    let env_guard = EnvGuard {
        home: std::env::var_os("HOME"),
        opengoose_home: std::env::var_os("OPENGOOSE_HOME"),
        xdg_state_home: std::env::var_os("XDG_STATE_HOME"),
        cwd: std::env::current_dir().unwrap(),
    };

    let state_home = tmp.path().join("state");
    std::fs::create_dir_all(&state_home).unwrap();

    unsafe {
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("OPENGOOSE_HOME", tmp.path());
        std::env::set_var("XDG_STATE_HOME", &state_home);
        std::env::set_current_dir(tmp.path()).unwrap();
    }

    f().await;
    // env_guard Drop handles restoration — even on panic
    drop(env_guard);
    drop(guard);
}
```

- [ ] **Step 3: Run all skills tests**

Run: `cargo test -p opengoose -- skills -v`
Expected: ALL 7 tests using `with_clean_home()` pass

- [ ] **Step 4: Commit**

```
fix: make with_clean_home() panic-safe via Drop guard
```

---

## Task 4: Hermetic log_entry tests

**Files:**
- Modify: `crates/opengoose/src/tui/log_entry.rs` (test module, lines 195-211)

- [ ] **Step 1: Add mutex and rewrite tests**

In `crates/opengoose/src/tui/log_entry.rs` test module, add a mutex and rewrite the two non-hermetic tests:

```rust
static LOG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn create_session_log_file_creates_file_in_home() {
    let guard = LOG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let prev = std::env::var_os("HOME");
    unsafe { std::env::set_var("HOME", tmp.path()); }

    let file = create_session_log_file();

    match prev {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    assert!(file.is_ok(), "should create session log file: {:?}", file.err());
    drop(guard);
}

#[test]
fn cleanup_old_logs_runs_against_temp_home() {
    let guard = LOG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let prev = std::env::var_os("HOME");
    unsafe { std::env::set_var("HOME", tmp.path()); }

    let result = cleanup_old_logs(100);

    match prev {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    assert!(result.is_ok());
    drop(guard);
}
```

- [ ] **Step 2: Run log_entry tests**

Run: `cargo test -p opengoose -- log_entry -v`
Expected: all pass

- [ ] **Step 3: Commit**

```
fix: make log_entry tests hermetic with tempdir HOME isolation
```

---

## Task 5: middleware.rs functional core extraction + npm fake

**Files:**
- Modify: `crates/opengoose-rig/src/middleware.rs` (production code lines 10-24, test module)

### Step 5a: Extract hydration_context

- [ ] **Step 5a-1: Write tests for hydration_context**

Add to the test module in `crates/opengoose-rig/src/middleware.rs`:

```rust
#[test]
fn hydration_context_includes_agents_md_and_catalog() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("AGENTS.md"), "be helpful").unwrap();
    let ctx = hydration_context(tmp.path(), "## Skills\n- skill-a");
    assert_eq!(ctx.len(), 2);
    assert_eq!(ctx[0], ("agents-md".into(), "be helpful".into()));
    assert_eq!(ctx[1], ("skill-catalog".into(), "## Skills\n- skill-a".into()));
}

#[test]
fn hydration_context_skips_missing_agents_md_and_empty_catalog() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = hydration_context(tmp.path(), "");
    assert!(ctx.is_empty());
}

#[test]
fn hydration_context_includes_only_catalog_when_no_agents_md() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = hydration_context(tmp.path(), "## Skills");
    assert_eq!(ctx.len(), 1);
    assert_eq!(ctx[0].0, "skill-catalog");
}

#[test]
fn hydration_context_includes_only_agents_md_when_catalog_empty() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("AGENTS.md"), "instructions").unwrap();
    let ctx = hydration_context(tmp.path(), "");
    assert_eq!(ctx.len(), 1);
    assert_eq!(ctx[0], ("agents-md".into(), "instructions".into()));
}
```

- [ ] **Step 5a-2: Run tests to verify they fail**

Run: `cargo test -p opengoose-rig -- hydration_context -v`
Expected: FAIL — `hydration_context` not found

- [ ] **Step 5a-3: Extract hydration_context function**

In `crates/opengoose-rig/src/middleware.rs`, refactor `pre_hydrate` (lines 10-24):

Replace the existing `pre_hydrate` with:

```rust
fn hydration_context(work_dir: &Path, skill_catalog: &str) -> Vec<(String, String)> {
    let mut ctx = Vec::new();
    if let Some(agents_md) = load_agents_md(work_dir) {
        ctx.push(("agents-md".to_string(), agents_md));
    }
    if !skill_catalog.is_empty() {
        ctx.push(("skill-catalog".to_string(), skill_catalog.to_string()));
    }
    ctx
}

pub async fn pre_hydrate(agent: &Agent, work_dir: &Path, skill_catalog: &str) {
    for (key, value) in hydration_context(work_dir, skill_catalog) {
        agent.extend_system_prompt(key, value).await;
    }
}
```

- [ ] **Step 5a-4: Run tests to verify they pass**

Run: `cargo test -p opengoose-rig -- hydration_context -v`
Expected: ALL 4 new tests pass

- [ ] **Step 5a-5: Remove old smoke-only pre_hydrate tests**

Delete `pre_hydrate_with_agents_md_and_nonempty_catalog` and `pre_hydrate_with_empty_catalog_and_no_agents_md` tests — they are superseded by the new `hydration_context` tests which actually assert behavior.

- [ ] **Step 5a-6: Run all middleware tests**

Run: `cargo test -p opengoose-rig -- middleware -v`
Expected: all pass

### Step 5b: Deterministic npm tests

- [ ] **Step 5b-1: Rewrite npm success test with fake script**

Replace `post_execute_calls_npm_check_when_package_json_present` with:

```rust
#[tokio::test]
async fn post_execute_npm_check_succeeds_with_fake_npm() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"name":"test","scripts":{"test":"echo ok"}}"#,
    )
    .unwrap();

    // Create fake npm that succeeds
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let fake_npm = bin_dir.join("npm");
    std::fs::write(&fake_npm, "#!/bin/sh\nexit 0").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_npm, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // Prepend fake npm to PATH
    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), orig_path.to_string_lossy());
    unsafe { std::env::set_var("PATH", &new_path); }

    let result = post_execute(tmp.path()).await;
    assert!(result.is_none(), "successful npm test should return None");

    unsafe { std::env::set_var("PATH", &orig_path); }
}
```

- [ ] **Step 5b-2: Rewrite npm failure test with fake script**

Replace `post_execute_npm_check_returns_error_on_failure` with:

```rust
#[tokio::test]
async fn post_execute_npm_check_reports_failure_with_fake_npm() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"name":"test","scripts":{"test":"exit 1"}}"#,
    )
    .unwrap();

    // Create fake npm that fails
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let fake_npm = bin_dir.join("npm");
    std::fs::write(&fake_npm, "#!/bin/sh\necho 'test failed' >&2; exit 1").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_npm, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), orig_path.to_string_lossy());
    unsafe { std::env::set_var("PATH", &new_path); }

    let result = post_execute(tmp.path()).await;
    assert!(result.is_some(), "failed npm test should return Some");
    assert!(result.unwrap().contains("npm test failed"));

    unsafe { std::env::set_var("PATH", &orig_path); }
}
```

- [ ] **Step 5b-3: Run all middleware tests**

Run: `cargo test -p opengoose-rig -- middleware -v`
Expected: all pass

- [ ] **Step 5b-4: Fix naming — rename `_result` to `result` if any remain**

Check for any `let _result = ` in the test module and rename to `result`.

- [ ] **Step 5b-5: Commit**

```
refactor: extract hydration_context pure function, add deterministic npm tests
```

---

## Task 6: list.rs global_only test isolation

**Files:**
- Modify: `crates/opengoose-skills/src/manage/list.rs` (test around line 361-384)

- [ ] **Step 1: Rewrite test with separate home/project dirs**

Replace `run_with_global_only_skips_project_skills` in `crates/opengoose-skills/src/manage/list.rs`:

```rust
#[test]
fn run_with_global_only_skips_project_skills() {
    let home_tmp = tempfile::tempdir().unwrap();
    let project_tmp = tempfile::tempdir().unwrap();
    let env = crate::test_utils::IsolatedEnv::new(home_tmp.path());
    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(project_tmp.path()).unwrap();

    // Global skill in HOME
    let global_dir = home_tmp
        .path()
        .join(".opengoose/skills/installed/g-skill");
    std::fs::create_dir_all(&global_dir).unwrap();
    std::fs::write(
        global_dir.join("SKILL.md"),
        "---\nname: g-skill\ndescription: Global\n---\n",
    )
    .unwrap();

    // Project skill in CWD (separate from HOME)
    let project_dir = project_tmp
        .path()
        .join(".opengoose/skills/learned/p-skill");
    write_metadata(&project_dir);

    // Both visible when global_only=false
    assert!(run(home_tmp.path(), false, false).is_ok());

    // Only global visible when global_only=true
    assert!(run(home_tmp.path(), true, false).is_ok());

    std::env::set_current_dir(cwd).unwrap();
    drop(env);
}
```

- [ ] **Step 2: Fix naming — rename any `_env` to `env` in the test module**

Check all tests in list.rs for `let _env =` and rename to `let env =`.

- [ ] **Step 3: Run list tests**

Run: `cargo test -p opengoose-skills -- list -v`
Expected: all pass

- [ ] **Step 4: Commit**

```
fix: use separate home/project dirs in global_only test for real isolation
```

---

## Task 7: web/mod.rs poll-based server connect

**Files:**
- Modify: `crates/opengoose/src/web/mod.rs` (test module)

- [ ] **Step 1: Add helper function for poll-based connect**

In the `#[cfg(test)]` module of `crates/opengoose/src/web/mod.rs`, add a helper:

```rust
async fn connect_with_retry(port: u16) -> tokio::net::TcpStream {
    let start = std::time::Instant::now();
    loop {
        match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            Ok(s) => return s,
            Err(e) if start.elapsed() < std::time::Duration::from_secs(2) => {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Err(e) => panic!("server did not start within 2s: {e}"),
        }
    }
}
```

- [ ] **Step 2: Replace fixed sleeps with connect_with_retry**

In `spawn_server_binds_and_serves_index` test (~line 75-96):

Replace:
```rust
spawn_server(board.clone(), port).await.unwrap();
tokio::time::sleep(std::time::Duration::from_millis(100)).await;
let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
```

With:
```rust
spawn_server(board.clone(), port).await.unwrap();
let mut stream = connect_with_retry(port).await;
```

In `board_notify_triggers_sse_event` test (~line 101-162):

Replace:
```rust
spawn_server(board.clone(), port).await.unwrap();
tokio::time::sleep(std::time::Duration::from_millis(50)).await;
```

With:
```rust
spawn_server(board.clone(), port).await.unwrap();
// Use connect_with_retry for the SSE stream too
```

Note: The SSE test may use a different connection method. Apply `connect_with_retry` where `TcpStream::connect` is used. For the SSE connection that uses a different client, just remove the `sleep` and use the retry pattern inline if needed.

- [ ] **Step 3: Fix naming in the test module**

Check for any `_` prefix variables and rename.

- [ ] **Step 4: Run web tests**

Run: `cargo test -p opengoose -- web -v`
Expected: all pass

- [ ] **Step 5: Commit**

```
fix: replace fixed sleep with poll-based connect in web server tests
```

---

## Task 8: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass across all crates

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Create PR**

Branch: `fix/pr340-review-followup`

PR title: `fix: address PR #340 review feedback — trait DI, test isolation, bug fixes`

PR body should reference the original PR #340 and list the 7 changes.
