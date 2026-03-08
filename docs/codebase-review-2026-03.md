# Codebase Review — March 2026 (rev3)

This document is the living architectural reference for opengoose. Update it
whenever P0/P1 items change status or the dependency graph changes.

---

## Project Summary

OpenGoose is a Goose-native, multi-channel AI orchestrator written in Rust.
It routes messages from messaging platforms (Discord, Slack, Telegram, and
custom platforms) through a shared engine that delegates to either a
team-based orchestrator or the Goose single-agent handler.

---

## 12-Crate Dependency Graph

```
opengoose-types          (no opengoose deps — shared types, Platform, SessionKey, events)
opengoose-secrets        (no opengoose deps — keyring / env credential storage)

opengoose-profiles       ← types
opengoose-persistence    ← types
opengoose-provider-bridge← secrets

opengoose-teams          ← types, profiles, persistence

opengoose-core           ← types, profiles, teams, persistence
                           (Engine, GatewayBridge, split_message, StreamResponder)

opengoose-discord        ← types, core      (Discord channel adapter)
opengoose-slack          ← types, core      (Slack channel adapter)
opengoose-telegram       ← types, core      (Telegram channel adapter)
opengoose-tui            ← types, secrets, provider-bridge, teams  (Ratatui TUI)

opengoose-cli            ← everything above (binary: `opengoose`)
```

Layer ordering: types/secrets → profiles/persistence/provider-bridge → teams →
core → adapters/tui → cli.

---

## Architecture Principles

1. **Prefer Goose-native reuse** — delegate to Goose APIs wherever possible;
   avoid reimplementing agent execution logic.
2. **Core stays small and explicit** — business logic shared across adapters
   lives in `opengoose-core`; channel specifics stay in their adapter crate.
3. **Transport / platform specifics inside adapter crates** — `GatewayBridge`
   in core provides the unified orchestration API; each adapter calls it.
4. **Testability** — policy logic (Engine, SessionManager, TeamOrchestrator)
   is separated from I/O plumbing.

---

## Key Subsystems

### GatewayBridge (`opengoose-core::bridge`)

Shared orchestration bridge used by all channel gateways. Centralises:

- `relay_and_drive_stream()` — combines message relaying + streaming
  orchestration in one call, eliminating per-adapter boilerplate.
- Error event emission — emits `AppEventKind::Error` centrally so adapters do
  not need to handle it individually.
- `on_outgoing_message()` — returns the decoded `SessionKey`, eliminating
  duplicate `from_stable_id` calls in Discord, Slack, and Telegram adapters.
- `on_start(handler)` — called by `Gateway::start()`; stores the handler and
  emits `AppEventKind::GooseReady`.

### Engine (`opengoose-core::engine`)

Platform-agnostic core engine. Routes messages to team orchestration or the
Goose single-agent handler. Owns a cached `SessionStore` (created once at
initialization) for consistent cache locality across calls.

### SessionManager (`opengoose-core`)

Manages active team sessions per user. Stores a `SessionStore` instance as a
field (created once at construction) so `set_active_team()`,
`clear_active_team()`, and `get_active_team()` reuse the same instance across
calls — no redundant allocations per method invocation.

### ExecutorContext (`opengoose-teams::executor_context`)

Shared execution context used by all three executor types (`ChainExecutor`,
`FanOutExecutor`, `RouterExecutor`). Eliminates struct duplication across
executors and provides:

- `ExecutorContext<'a>` — holds `team`, `profile_store`, and `pool` references.
- `resolve_profile(store, name)` — uniform profile lookup with consistent error
  message (`"profile \`{name}\` not found"`).
- `inject_team_role(runner, role)` — standardised role injection via
  `extend_system_prompt("team_role", "Your role: {role}")` across all executors.

### Platform enum (`opengoose-types`)

```rust
pub enum Platform {
    Discord,
    Telegram,
    Slack,
    /// Supports new platforms without modifying this crate.
    Custom(String),
}
```

- `Platform::from_str_lossy(s)` — accepts any string; returns `Custom` for
  unknown names. Use this when accepting user-supplied platform identifiers.
- `Platform::from_str_opt(s)` — strict; returns `None` for unknown names.
  Used by `SessionKey::from_stable_id` to distinguish known platform prefixes.

### message_utils (`opengoose-core::message_utils`)

- `split_message(text, max_bytes)` — UTF-8-safe message splitter shared by all
  channel adapters. Adapters import from here; no local copies.
- `truncate_for_display(text, max_chars)` — display truncation helper.

---

## P0 Items — Completed

| Item | PR | Notes |
|------|----|-------|
| Unify `split_message` into core | [#41][pr41], [#42][pr42] | Adapters import from `opengoose_core::message_utils` |
| `GatewayBridge::relay_and_drive_stream()` | [#41][pr41] | Eliminates per-adapter streaming boilerplate |
| `Platform::Custom(String)` variant | [#41][pr41] | Custom platforms without core changes |
| Centralise error event emission in bridge | [#44][pr44] | Adapters no longer emit `AppEventKind::Error` |
| `on_outgoing_message()` returns `SessionKey` | [#44][pr44] | Removes duplicate `from_stable_id` calls in adapters |
| `SessionStore` cached in `Engine` | [#44][pr44] | Single instance per Engine lifetime |
| `SessionStore` stored in `SessionManager` | [#46][pr46] | Eliminates per-call `SessionStore::new()` in `set/clear/get_active_team` |
| Remove legacy `OpenGooseGateway` / `DiscordAdapter` | [#41][pr41] | Team command handling moved to `Engine::handle_team_command()` |
| Extract `ExecutorContext` in `opengoose-teams` | [#62][pr62] | Shared struct + `resolve_profile` + `inject_team_role` helpers; standardises role string across all executors |

[pr41]: https://github.com/soilSpoon/opengoose/pull/41
[pr42]: https://github.com/soilSpoon/opengoose/pull/42
[pr44]: https://github.com/soilSpoon/opengoose/pull/44
[pr46]: https://github.com/soilSpoon/opengoose/pull/46
[pr62]: https://github.com/soilSpoon/opengoose/pull/62

---

## P1 Backlog

| Item | DoD |
|------|-----|
| Gateway factory pattern | Single `build_gateways()` fn in cli replaces per-adapter construction boilerplate |
| `finalize_draft` consolidation | Single `StreamResponder::finalize` used by all adapters; no local flush logic |
| Pairing router | `GatewayBridge` owns pairing-code routing; adapters call one method |
| TUI refactoring | Credential flow, event handler, and state modules each < 300 LOC; tests cover all state transitions |

---

## Adding a New Channel Platform

1. Create `opengoose-<platform>` crate depending on `opengoose-types` and
   `opengoose-core`.
2. Implement the Goose `Gateway` trait.
3. Construct a `GatewayBridge` and call `bridge.on_start()` /
   `bridge.relay_and_drive_stream()` in your gateway implementation.
4. Use `Platform::from_str_lossy("<platform>")` or add a dedicated `Platform`
   variant if the platform warrants first-class status.
5. Wire the gateway into `opengoose-cli::cmd::run`.
6. Add the crate to workspace `Cargo.toml` members and `README.md`.

No changes to `opengoose-core` or `opengoose-types` are required for steps 1–3
thanks to `Platform::Custom` and `GatewayBridge`.
