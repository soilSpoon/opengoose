# PR #340 Review Follow-up — Design Spec

**Date:** 2026-03-21
**PR:** https://github.com/soilSpoon/opengoose/pull/340
**Status:** Merged to main. This is a follow-up PR.

## Context

PR #340 (test coverage sweep) received CodeRabbit review comments. 3 were addressed in-PR; the remaining items need a follow-up. This spec covers all unresolved issues.

## Changes

### 1. Trait DI for `evolver.rs` — Remove test code from production

**Problem:** `call_agent()` uses `cfg!(test)` (runtime check) + `OPENGOOSE_TEST_CALL_AGENT` env var to mock LLM calls. This embeds test infrastructure in the production binary and relies on global mutable state (env vars + mutex) for test isolation.

**Solution:** Extract an `AgentCaller` trait as the seam between business logic and LLM IO.

```rust
// Production trait + implementation
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
        // Stream collection logic moves here from old call_agent()
        let message = Message::user().with_text(prompt);
        let session_config = SessionConfig { id: format!("evolve-{work_id}"), ... };
        let stream = self.agent.reply(message, session_config, None).await?;
        // collect stream into String
    }
}
```

```rust
// Test-only mock
#[cfg(test)]
struct MockAgentCaller {
    reply: String,
}

#[cfg(test)]
#[async_trait]
impl AgentCaller for MockAgentCaller {
    async fn call(&self, prompt: &str, work_id: i64) -> anyhow::Result<String> {
        // || split + ERR: protocol moves here
        let raw = if prompt.contains("Previous output had format errors") {
            self.reply.split("||").nth(1).unwrap_or(&self.reply).to_string()
        } else {
            self.reply.split("||").next().unwrap_or(&self.reply).to_string()
        };
        if let Some(err_msg) = raw.strip_prefix("ERR:") {
            return Err(anyhow::anyhow!(err_msg.to_string()));
        }
        Ok(raw)
    }
}
```

**Signature changes:**

| Function | Before | After |
|----------|--------|-------|
| `process_stamp` | `(board, agent: &Agent, stamp)` | `(board, caller: &dyn AgentCaller, stamp)` |
| `execute_action` | `(..., agent: &Agent, ...)` | `(..., caller: &dyn AgentCaller, ...)` |
| `run_sweep` | `(board, agent: &Agent)` | `(board, caller: &dyn AgentCaller)` |
| `call_agent` | free function | **deleted** — absorbed into `RealAgentCaller::call()` |

**Entry point change** in `run()`:

`run()` lazily initializes `Agent` as `Option<Agent>`. `RealAgentCaller` is created
at each usage point after agent exists, not upfront:

```rust
// In the stamp processing loop (after lazy init):
let caller = RealAgentCaller { agent: agent.as_ref().unwrap() };
process_stamp(&board, &caller, stamp).await?;

// In the sweep branch (agent already exists via `if let Some(ref agent)`):
let caller = RealAgentCaller { agent };
run_sweep(&board, &caller).await?;
```

**Test migration (20 tests):**
```rust
// Before:
let prev_reply = set_env_var("OPENGOOSE_TEST_CALL_AGENT", Some("..."));
let agent = Agent::new();
process_stamp(&board, &agent, &stamp).await.unwrap();
restore_env_var("OPENGOOSE_TEST_CALL_AGENT", prev_reply);

// After:
let caller = MockAgentCaller { reply: "...".into() };
process_stamp(&board, &caller, &stamp).await.unwrap();
```

No more `Agent::new()` dummy, no env vars, no mutex for agent mocking.

**Cleanup:** Remove `set_env_var()`, `restore_env_var()` helpers if they become unused.
The `test_env_lock()` mutex may still be needed for other env-var-based tests in the
evolver module (e.g., HOME). Keep if referenced; remove if not.

### 2. `EvolveMode::session_for` — Honor session_id contract

**Problem:** `WorkInput.session_id` doc says "설정 시 WorkMode.session_for()보다 우선" but `EvolveMode` ignores it. `TaskMode` correctly checks it.

**Fix:**
```rust
impl WorkMode for EvolveMode {
    fn session_for(&self, input: &WorkInput) -> String {
        if let Some(id) = &input.session_id {
            return id.clone();
        }
        match input.work_id {
            Some(id) => format!("evolve-{id}"),
            None => format!("evolve-{}", /* timestamp */),
        }
    }
}
```

**Test:**
```rust
#[test]
fn evolve_mode_uses_presupplied_session_id() {
    let mode = EvolveMode;
    let input = WorkInput::chat("x").with_session_id("pre-evolve".into());
    assert_eq!(mode.session_for(&input), "pre-evolve");
}
```

### 3. `with_clean_home()` — Panic-safe env restoration

**Problem:** If assertion inside `f().await` panics, manual cleanup block never runs. Env vars leak to subsequent tests.

**Fix:** Use Drop guard pattern (already established in `opengoose-skills::test_utils::IsolatedEnv`).

```rust
struct EnvGuard {
    home: Option<OsString>,
    opengoose_home: Option<OsString>,
    xdg_state_home: Option<OsString>,
    cwd: PathBuf,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            // same for opengoose_home, xdg_state_home
            std::env::set_current_dir(&self.cwd).unwrap();
        }
    }
}
```

Remove manual cleanup block after `f().await`.

### 4. `log_entry.rs` — Hermetic tests

**Problem:** Tests call `create_session_log_file()` and `cleanup_old_logs()` against real `$HOME`. Can modify real user files.

**Fix:** `dirs::home_dir()` reads `HOME` env var on Unix. Set `HOME` to tempdir.
Use mutex for serialization since multiple tests modify `HOME`.

```rust
static LOG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn create_session_log_file_creates_file_in_home() {
    let guard = LOG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let prev = std::env::var_os("HOME");
    unsafe { std::env::set_var("HOME", tmp.path()); }
    let file = create_session_log_file();
    // restore
    match prev {
        Some(v) => unsafe { std::env::set_var("HOME", v) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    assert!(file.is_ok());
}
```

Same pattern for `cleanup_old_logs_runs_against_home_dir`.

### 5. `middleware.rs` — Strengthen test assertions

**Problem:** `pre_hydrate` tests only assert "no panic". npm tests accept any result.

**Fix for pre_hydrate — Functional Core extraction:**

`pre_hydrate()` returns `()` and calls `agent.extend_system_prompt()` which has no
inspectable output. Instead of trying to assert on the opaque Agent, extract the pure
data-preparation logic into a testable function:

```rust
/// Pure function: compute what should be injected into system prompt.
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

/// Thin IO shell — applies hydration context to agent.
pub async fn pre_hydrate(agent: &Agent, work_dir: &Path, skill_catalog: &str) {
    for (key, value) in hydration_context(work_dir, skill_catalog) {
        agent.extend_system_prompt(key, value).await;
    }
}
```

Tests assert on `hydration_context()` (pure, deterministic):
```rust
#[test]
fn hydration_context_includes_agents_md_when_present() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("AGENTS.md"), "be helpful").unwrap();
    let ctx = hydration_context(tmp.path(), "## Skills\n- skill-a");
    assert_eq!(ctx.len(), 2);
    assert_eq!(ctx[0], ("agents-md".into(), "be helpful".into()));
    assert_eq!(ctx[1], ("skill-catalog".into(), "## Skills\n- skill-a".into()));
}

#[test]
fn hydration_context_skips_empty_catalog_and_missing_agents_md() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = hydration_context(tmp.path(), "");
    assert!(ctx.is_empty());
}
```

**Fix for npm:** Create a fake `npm` script in tempdir, prepend to PATH:
```rust
// Success case:
let fake_npm = tmp.join("npm");
std::fs::write(&fake_npm, "#!/bin/sh\necho 'ok'")?;
// chmod +x, prepend to PATH
// assert post_execute returns None (success = no error)

// Failure case:
std::fs::write(&fake_npm, "#!/bin/sh\necho 'fail' >&2; exit 1")?;
// assert post_execute returns Some("npm test failed:...")
```

### 6. `list.rs` — Separate home/project dirs for global_only test

**Problem:** `IsolatedEnv::new(tmp)` sets HOME=tmp, test also sets CWD=tmp. So global and project skill dirs resolve under the same root — doesn't prove global_only excludes project skills.

**Fix:** Use two separate tempdirs:
```rust
let home_tmp = tempfile::tempdir().unwrap();
let project_tmp = tempfile::tempdir().unwrap();
let env = IsolatedEnv::new(home_tmp.path());
env::set_current_dir(project_tmp.path()).unwrap();
// create global skill in home_tmp, project skill in project_tmp
// assert global_only=true only finds global skill
```

### 7. `web/mod.rs` — Replace fixed sleep with poll

**Problem:** `sleep(50-100ms)` before TCP connect is timing-dependent. Flaky on slow CI.

**Fix:** Retry loop with timeout:
```rust
let start = Instant::now();
let stream = loop {
    match TcpStream::connect(("127.0.0.1", port)).await {
        Ok(s) => break s,
        Err(_) if start.elapsed() < Duration::from_secs(2) => {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(e) => panic!("server did not start within 2s: {e}"),
    }
};
```

## Out of Scope

- `source.rs` `cfg!(test)` pattern — same issue as evolver but lower priority. Separate PR.
- Adding `scopeguard` crate dependency — use manual Drop impls instead.

## Testing

- `cargo test -p opengoose` — evolver tests, skills tests, web tests, tui tests
- `cargo test -p opengoose-rig` — middleware tests, work_mode tests
- `cargo test -p opengoose-skills` — list tests
- All existing tests must pass with the new patterns.
