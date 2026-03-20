# Worker Pull Loop Integration

## Context

OpenGoose v0.2 has a pull architecture where Worker rigs autonomously claim work from the Board. The `Rig<TaskMode>` (Worker) and `Worker::run()` pull loop are implemented but never spawned. The main binary uses a raw Goose `Agent` directly, bypassing the Rig abstraction entirely.

This spec wires the existing pieces together so the Board becomes a live orchestrator.

## Decisions

1. **Chat/task separation**: Operator handles conversation directly. Tasks reach the Board only through explicit action.
2. **Worker count**: One auto-spawned Worker (`id="worker"`). Multi-worker expansion deferred.
3. **Board posting**: Two paths — `/task` TUI command and Agent tool use via Board CLI.

## Changes

### 1. main.rs — replace raw Agent with Operator + Worker

**Current**: `create_agent()` returns `(Agent, String)`. TUI and headless modes call Agent directly. Agent creation logic is duplicated across `main.rs::create_agent()` and `evolver.rs::create_evolver_agent()`.

**Target**:
- Extract common Agent creation into `create_base_agent(session_name)` → `(Agent, String)`.
- Role-specific wrappers call base + `extend_system_prompt`:
  - `create_operator_agent()` — Board CLI instructions (existing prompt)
  - `create_worker_agent()` — task-oriented prompt
  - `evolver.rs::create_evolver_agent()` — refactor to call base (deferred, not blocking)
- Wrap Operator Agent in `Operator::without_board()` for chat.
- Create Worker Agent, wrap in `Rig::new()` with `TaskMode`.
- `tokio::spawn(worker.run())` alongside the existing Evolver spawn.
- `Rig<M>` must be `Send + Sync` for `tokio::spawn`. It is — `Agent`, `Arc<Board>`, `CancellationToken`, `RigId`, `TaskMode` are all `Send + Sync`.

```rust
// Common agent creation
async fn create_base_agent(session_name: &str) -> Result<(Agent, String)> { ... }

// Operator — chat
let (agent, session_id) = create_operator_agent().await?;
let operator = Arc::new(Operator::without_board(
    RigId::new("operator"),
    agent,
    &session_id,
));

// Worker — pull loop
let (worker_agent, _) = create_worker_agent().await?;
let worker = Arc::new(Worker::new(
    RigId::new("worker"),
    Arc::clone(&board),
    worker_agent,
    TaskMode,
));
let worker_cancel = worker.cancel_token();
tokio::spawn({
    let w = Arc::clone(&worker);
    async move { w.run().await }
});
```

### 2. TUI — Operator streaming + /task command

**Current**: `tui::run_tui(board, agent, session_id)` takes a raw `Arc<Agent>`. `/task` handler (event.rs:271-281) posts to Board then tells the Operator Agent to claim/submit manually.

**Target**:
- Accept `Arc<Operator>` instead of `Arc<Agent>`.
- **Streaming**: TUI needs per-token streaming for chat display. `Operator.chat()` hides the stream internally. Solution: add a streaming method to Operator that returns `AgentEvent`s to callers via a channel or by returning the stream directly. This keeps session management centralized and gives TUI proper streaming access.

```rust
// In Rig<M> (shared) or Operator (specific):
pub async fn process_streaming(&self, input: WorkInput)
    -> anyhow::Result<impl Stream<Item = Result<AgentEvent>>>
{
    let session_config = self.mode.session_config(&input);
    let message = Message::user().with_text(&input.text);
    self.agent.reply(message, session_config, Some(self.cancel.clone())).await
}

// Operator convenience:
pub async fn chat_streaming(&self, input: &str)
    -> anyhow::Result<impl Stream<Item = Result<AgentEvent>>>
{
    self.process_streaming(WorkInput::chat(input)).await
}
```

TUI calls `operator.chat_streaming(input)` and consumes the stream for per-token display. No raw Agent access needed.
- **`/task` handler change**: Remove the "notify Agent to claim" block (event.rs:271-281). Worker now picks up Board items automatically. Only post + print confirmation.
- Worker progress not shown in TUI (out of scope — just Board status via 2-second tick).

### 3. Headless mode (Commands::Run)

**Current**: Posts to Board then runs Agent directly via `run_agent_streaming`.

**Target**:
- Post to Board, **capture the returned `item.id`**.
- Use Board `notify` (not polling) to detect completion. `board.submit()` calls `notify.notify_waiters()`, so headless mode wakes immediately when the Worker finishes.
- Register `notified()` before checking status (same pattern as Worker pull loop — no notify loss).
- Handle `Option::None` (item deleted) as an error.
- 10-minute timeout via `tokio::time::sleep`.
- Wrap in `tokio::select!` with `ctrl_c` (preserving current pattern).
- Print Worker's conversation log after completion.

```rust
let item = board.post(PostWorkItem { ... }).await?;
let timeout = tokio::time::sleep(Duration::from_secs(600));
tokio::pin!(timeout);

loop {
    let notified = board.notify_handle().notified();

    match board.get(item.id).await? {
        Some(wi) if wi.status == Status::Done => break,
        Some(_) => {}
        None => anyhow::bail!("work item #{} was deleted", item.id),
    }

    tokio::select! {
        _ = notified => {}
        _ = &mut timeout => anyhow::bail!("timed out waiting for work item #{}", item.id),
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nInterrupted.");
            return Ok(());
        }
    }
}
```

### 4. Worker Agent system prompt

`create_worker_agent()` calls `create_base_agent("worker")` then `extend_system_prompt().await` with:

```
You are an OpenGoose Worker rig. You receive tasks from the Board and execute them autonomously.
Focus on completing the task. Use available tools. Do not ask clarifying questions — make reasonable assumptions and proceed.
```

Note: `extend_system_prompt` is async and must be awaited.

### 5. Worker failure — Board cleanup

In `Worker::try_claim_and_execute()` (rig.rs), if `self.process(input).await` fails, the work item stays in `Claimed` status permanently. Add `board.abandon(item.id)` on process failure:

```rust
async fn try_claim_and_execute(&self) -> anyhow::Result<()> {
    // ... claim ...
    let result = self.process(input).await;
    if let Err(e) = &result {
        warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
        board_arc.abandon(item.id).await.ok();
    } else {
        board_arc.submit(item.id, &self.id).await?;
    }
    result
}
```

### 6. Notify loss prevention — register-before-check pattern

`tokio::sync::Notify::notify_waiters()` only wakes tasks currently awaiting `.notified()`. Fix: register the `notified()` future **before** checking for work. Any notification arriving during execution is captured by the already-registered future.

Also, `try_claim_and_execute` needs to distinguish "found work" from "no work available" so the loop knows whether to wait or immediately re-check.

```rust
// In Worker::run()
loop {
    // 1. Register interest FIRST — captures any notify from this point on
    let notified = board.notify_handle().notified();

    // 2. Check for ready items + execute
    match self.try_claim_and_execute().await {
        Ok(true) => continue,  // found work, check for more immediately
        Ok(false) => {}        // no work, fall through to wait
        Err(e) => warn!(rig = %self.id, error = %e, "execution failed"),
    }

    // 3. No work — wait for notification (no loss possible)
    tokio::select! {
        _ = notified => {}
        _ = self.cancel.cancelled() => break,
    }
}
```

Return type change for `try_claim_and_execute`: `Result<bool>` where `true` = claimed and executed, `false` = nothing ready.

### 7. Graceful shutdown

On TUI exit or headless completion, cancel the Worker via its `CancellationToken`:

```rust
worker.cancel(); // triggers break in Worker::run() loop
```

## Out of Scope

- Multi-worker spawning (rig-per-worker)
- Tag-based work item routing
- TUI live Worker output streaming
- Worker retry/backoff on repeated failure
- Operator → Board automatic routing (AI judges chat vs task)

## Testing

- Existing tests (10/10) must pass.
- New unit test: `try_claim_and_execute` calls `abandon` on process failure.
- Manual smoke test: `opengoose` → TUI → `/task "echo hello"` → verify Board status shows claimed → done.
- Manual smoke test: `opengoose run "echo hello"` → verify Worker executes and exits.
