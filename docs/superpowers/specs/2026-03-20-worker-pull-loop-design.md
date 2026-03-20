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

**Current**: `create_agent()` returns `(Agent, String)`. TUI and headless modes call Agent directly.

**Target**:
- Create a shared `Agent` and `Board`.
- Wrap the Agent in `Operator::without_board()` for chat.
- Create a second `Agent` for the Worker, wrap in `Rig::new()` with `TaskMode`.
- `tokio::spawn(worker.run())` alongside the existing Evolver spawn.
- `Rig<M>` must be `Send + Sync` for `tokio::spawn`. It is — `Agent`, `Arc<Board>`, `CancellationToken`, `RigId`, `TaskMode` are all `Send + Sync`.

```rust
// Operator — chat
let (agent, session_id) = create_agent().await?;
let operator = Arc::new(Operator::without_board(
    RigId::new("operator"),
    agent,
    &session_id,
));

// Worker — pull loop
let (worker_agent, _) = create_agent_for_worker().await?;
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

**Current**: `tui::run_tui(board, agent, session_id)` takes a raw `Arc<Agent>`.

**Target**:
- Accept `Arc<Operator>` instead of `Arc<Agent>`.
- **Streaming**: TUI needs per-token streaming for chat display. `Operator.chat()` hides the stream internally. Solution: use `operator.agent()` (pub accessor on `Rig<M>`, rig.rs:98) to access the underlying Agent for streaming, while the Operator's `ChatMode` provides the session ID. This keeps session management centralized without breaking TUI streaming.
- `/task <text>` → `board.post(PostWorkItem { title: text, ... })`. Print "Task #N posted." confirmation.
- Worker progress not shown in TUI (out of scope — just Board status).

### 3. Headless mode (Commands::Run)

**Current**: Posts to Board then runs Agent directly via `run_agent_streaming`.

**Target**:
- Post to Board, **capture the returned `item.id`**.
- Poll `board.get(id)` every 2 seconds until `Status::Done`, with 10-minute timeout.
- Handle `Option::None` (item deleted) as an error.
- Wrap polling in `tokio::select!` with `ctrl_c` (preserving current pattern).
- Print Worker's conversation log after completion.

```rust
let item = board.post(PostWorkItem { ... }).await?;
let deadline = Instant::now() + Duration::from_secs(600);
loop {
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(2)) => {
            match board.get(item.id).await? {
                Some(wi) if wi.status == Status::Done => break,
                Some(_) => continue,
                None => anyhow::bail!("work item #{} was deleted", item.id),
            }
        }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nInterrupted.");
            return Ok(());
        }
    }
    if Instant::now() > deadline {
        anyhow::bail!("timed out waiting for work item #{}", item.id);
    }
}
```

### 4. Worker Agent system prompt

Reuse `create_agent()` logic but with a task-oriented system prompt. Create `create_agent_for_worker()` that calls `extend_system_prompt().await` with:

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

### 6. Notify loss prevention

`tokio::sync::Notify::notify_waiters()` only wakes tasks currently awaiting `.notified()`. If the Worker is busy executing when a new task is posted, the notification is lost. Add a fallback sweep (same pattern as Evolver):

```rust
// In Worker::run()
loop {
    let notify = board.notify_handle();
    tokio::select! {
        _ = notify.notified() => { ... }
        _ = tokio::time::sleep(Duration::from_secs(30)) => {
            // Fallback: check for ready items even without notification
            if let Err(e) = self.try_claim_and_execute().await {
                warn!(rig = %self.id, error = %e, "fallback sweep failed");
            }
        }
        _ = self.cancel.cancelled() => { break; }
    }
}
```

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
