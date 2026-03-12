# Subagent System

The Goose subagent system enables hierarchical delegation of tasks.

## Components

### TaskConfig
`crates/goose/src/agents/subagent_task_config.rs`
- Defines the environment for a subagent.
- Shares the parent's LLM provider and working directory.
- Inherits specific extensions.

### SubagentHandler
`crates/goose/src/agents/subagent_handler.rs`
- The execution engine for subagents.
- Manages the `Agent::reply()` stream.
- Forwards MCP notifications to the parent.

### Execution Flow
1. **Extraction**: System instructions and user task are pulled from the Recipe.
2. **Initialization**: A new `Agent` instance is created with the provided `TaskConfig`.
3. **Execution**: The agent starts a reply stream.
4. **Monitoring**: Parent listens via `on_message` callbacks and `notification_tx` for MCP events.

## Comparison: OpenGoose Fan-Out
OpenGoose's `FanOutExecutor` uses a similar pattern but spawns agents in parallel using `JoinSet`.

| Aspect | Goose | OpenGoose |
|--------|-------|-----------|
| Parallelism | Sequential (via Summon) | Parallel (via JoinSet) |
| Cancellation | CancellationToken | JoinSet::abort_all() |
| Events | MCP Notifications | [BROADCAST] prefix |
