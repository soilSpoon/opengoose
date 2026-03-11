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

## 14-Crate Dependency Graph

```
opengoose-types          (no opengoose deps — shared types, Platform, SessionKey, events)
opengoose-secrets        (no opengoose deps — keyring / env credential storage)

opengoose-profiles       ← types
opengoose-projects       ← types          (ProjectDefinition, ProjectStore, ProjectContext)
opengoose-persistence    ← types
opengoose-provider-bridge← secrets

opengoose-teams          ← types, profiles, projects, persistence

opengoose-core           ← types, profiles, teams, persistence
                           (Engine, GatewayBridge, split_message, StreamResponder,
                            ThrottlePolicy)

opengoose-discord        ← types, core      (Discord channel adapter)
opengoose-slack          ← types, core      (Slack channel adapter)
opengoose-telegram       ← types, core      (Telegram channel adapter)
opengoose-tui            ← types, secrets, provider-bridge, teams  (Ratatui TUI)
opengoose-web            ← types, profiles, teams, persistence     (Axum + Askama dashboard)

opengoose-cli            ← everything above (binary: `opengoose`)
```

Layer ordering: types/secrets → profiles/projects/persistence/provider-bridge → teams →
core → adapters/tui/web → cli.

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
default `main` profile via real-time streaming. Owns a cached `SessionStore`
(created once at initialization) for consistent cache locality across calls.

Primary API: `process_message_streaming(session_key, author, text)` — always
returns `Some(broadcast::Receiver<StreamChunk>)`. Streaming lifecycle:

1. Emits `AppEventKind::StreamStarted`.
2. If a team is active: runs `TeamOrchestrator` and sends the final response
   as a single `StreamChunk::Delta` followed by `StreamChunk::Done`.
3. If no team is active: spawns a background task that drives
   `stream_default_profile()` → `AgentRunner::run_streaming()`, forwarding
   provider text deltas as they arrive. The task emits `AppEventKind::ResponseSent`
   and `AppEventKind::StreamCompleted` on success.

Observability: `engine.rs`, `bridge.rs`, and `stream_orchestrator.rs` instrument
key paths with manual `info_span!`/`debug_span!` spans (not `#[instrument]` — that
causes async type-inference overflow). Spans carry structured fields:
`session_id`, `team_name`, `gateway_type`, `message_type`, `channel_id`.

Error handling: panics previously caused by `.expect()` on mutex acquisition and
cache lookups have been replaced with graceful propagation. `PersistenceError::LockPoisoned`
is returned when the database mutex is poisoned; a missing post-insert cache entry
propagates as an `anyhow` error instead of crashing the process.

### ThrottlePolicy (`opengoose-core::throttle`)

Per-platform rate limiter for streaming message edits. Adapters use this to
avoid hitting platform API limits when updating in-progress streaming responses.

| Constructor | Interval | Min delta |
|---|---|---|
| `ThrottlePolicy::discord()` | none (every chunk) | 0 bytes |
| `ThrottlePolicy::slack()` | 1.2 s | 80 bytes |
| `ThrottlePolicy::telegram()` | 1.0 s | 50 bytes |

`should_update(current_len)` returns `true` when both the time and byte-delta
thresholds are satisfied. Call `record_update(sent_len)` after each edit.

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

### Web Dashboard (`opengoose-web`)

The web crate is split by adapter boundary rather than keeping HTML routes and
API handlers in one module.

- `server.rs` owns `WebOptions` and the page-side `PageState`.
- `routes/mod.rs` composes the Axum surface from:
  - `routes/pages/` for HTML routes
  - `routes/health.rs` for the status page and health/metrics endpoints
  - `routes/api.rs` for JSON and websocket API routes
  - `routes/live.rs` for shared Datastar SSE stream builders used by page
    routes
- `live/` keeps dashboard refresh internals local to the web crate by splitting
  snapshot capture (`snapshot.rs`), event diff emission (`changes.rs`), and the
  polling watcher loop (`watcher.rs`) instead of leaving them in one 500+ line
  module.
- `routes/pages/dashboard.rs` owns dashboard rendering and Datastar SSE patch
  events.
- `routes/pages/remote_agents.rs` owns remote-agent page rendering, disconnect
  actions, and registry stream refresh.
- `routes/pages/catalog.rs` is now a thin façade over
  `routes/pages/catalog/pages.rs` (GET/detail/live handlers) and
  `routes/pages/catalog/actions.rs` (form/action handlers), while
  `routes/pages/catalog_forms.rs` owns page/query payloads and
  `routes/pages/catalog_templates.rs` owns Askama wrapper templates and shared
  fragment render helpers.
- `routes/pages/catalog/pages.rs` no longer uses route-definition macros for
  GET pages; instead it binds each catalog page through a `CatalogPageSpec`
  that couples title/nav metadata with the loader, detail fragment, and page
  template in one place. That removes the old drift risk where route labels,
  loaders, and Askama types could diverge across separate macro arguments.
- `data/views/` is split into domain modules (`shared`, `runs`, `automation`,
  `status`, plus the existing `agents`, `sessions`, and `teams`) so view-model
  definitions are grouped by responsibility instead of accumulating inside one
  giant file.

The rendering and interaction model is intentionally two-layered:

- `Askama` renders full pages and patchable fragments.
- `Datastar` owns live refresh and page actions through `data-init`,
  `data-on:*`, and SSE `datastar-patch-elements` responses.
- The dashboard now ships Datastar as a vendored static asset under
  `assets/vendor/` instead of depending on a runtime CDN fetch.
- `templates/components/live_monitor_shell_start.html` and
  `live_monitor_shell_end.html` hold the shared Datastar live-shell markup
  used by the dashboard, status board, remote-agent board, and live sessions
  rail so the stream state wiring is defined once.
- Thin intro wrappers for `status`, `remote-agents`, and `sessions` were
  removed; their live fragments now render either directly from the top-level
  page template or from a meaningful stream partial such as
  `partials/status_stream.html` instead of composing throwaway pass-through
  includes.
- `status` and `remote-agents` no longer assemble hero/banner copy in Askama
  with ad-hoc `{% let %}` glue. Their page models now carry typed
  `intro/banner/metric_grid` inputs so the Rust view-model layer owns the UI
  copy and block composition, leaving the templates to render structure.
- `dashboard` now follows the same pattern for its top-level live shell:
  hero intro, live banner, metric grid, and gateway panel metadata are built
  in Rust and rendered directly, instead of passing loosely-related
  `mode_label` / `stream_summary` / `snapshot_label` strings through Askama
  glue partials.
- The same pages now push panel-level structure into Rust as well:
  `status` renders typed component callouts plus a typed gateway panel, while
  `remote-agents` renders typed metadata/code panels for connection and
  handshake details. That removed the remaining thin live partials that only
  existed to bind `{% let %}` variables before delegating to shared markup.
- `routes/health.rs` listens to the shared app event bus for live status
  updates and keeps only a slower fallback sweep for quiet periods, while
  `routes/pages/remote_agents.rs` listens to `RemoteAgentRegistry`
  revision changes instead of polling every few seconds.
- `routes/pages/dashboard.rs` now follows the same event-driven pattern and
  only keeps a slower fallback sweep so time-based labels continue to advance.
- `routes/live.rs` centralises the repeated `initial patch -> live loop ->
  optional fallback sweep -> keep-alive` wiring so page modules vary mostly
  by their render function and event matcher instead of re-implementing SSE
  glue.

That keeps `assets/app.js` limited to local-only enhancements such as theme
toggle, searchable rails, and sortable tables, instead of networking or live
transport orchestration.

For fast regression checks, `scripts/web-smoke.sh` provides a repeatable route
and SSE handshake smoke test for the live dashboard pages. The legacy
`scripts/web-smoke-agent-browser.sh` name is kept as a thin wrapper so older
local habits do not break, but the check itself is now pure `curl` and can run
inside CI without a browser daemon.

Workflow launches, trigger test runs, and webhook-triggered executions all
reuse the shared web `EventBus` so live pages stay in sync with runs started
from either HTML actions or JSON API endpoints.

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
| Agent-native project system (`opengoose-projects`) | (this PR) | `ProjectDefinition` (YAML), `ProjectStore`, `ProjectContext`; per-project `cwd`, `goal`, `context_files` injected into every agent system prompt; `opengoose project` CLI commands (list/show/add/remove/init/run); `run_headless_with_project` in headless.rs; `TeamDefinition.goal` for team-level goal fallback |
| Unify `split_message` into core | [#41][pr41], [#42][pr42] | Adapters import from `opengoose_core::message_utils` |
| `GatewayBridge::relay_and_drive_stream()` | [#41][pr41] | Eliminates per-adapter streaming boilerplate |
| `Platform::Custom(String)` variant | [#41][pr41] | Custom platforms without core changes |
| Centralise error event emission in bridge | [#44][pr44] | Adapters no longer emit `AppEventKind::Error` |
| `on_outgoing_message()` returns `SessionKey` | [#44][pr44] | Removes duplicate `from_stable_id` calls in adapters |
| `SessionStore` cached in `Engine` | [#44][pr44] | Single instance per Engine lifetime |
| `SessionStore` stored in `SessionManager` | [#46][pr46] | Eliminates per-call `SessionStore::new()` in `set/clear/get_active_team` |
| Remove legacy `OpenGooseGateway` / `DiscordAdapter` | [#41][pr41] | Team command handling moved to `Engine::handle_team_command()` |
| Extract `ExecutorContext` in `opengoose-teams` | [#62][pr62] | Shared struct + `resolve_profile` + `inject_team_role` helpers; standardises role string across all executors |
| `Engine::process_message_streaming()` + `AgentRunner::run_streaming()` | [stream-commit][stream-commit] | Real-time streaming for both default-profile and team modes via `broadcast::Sender<StreamChunk>`; `ThrottlePolicy` added for per-platform edit rate-limiting |
| Manual tracing spans in core engine | [#104][pr104] | `info_span!`/`debug_span!` spans in `engine.rs`, `bridge.rs`, `stream_orchestrator.rs`; fields: `session_id`, `team_name`, `gateway_type`, `message_type`, `channel_id` |
| Graceful error handling (no more panics) | [ope-105][ope105] | `PersistenceError::LockPoisoned` replaces mutex `.expect()`; missing cache entry propagates as recoverable `anyhow` error |

[pr41]: https://github.com/soilSpoon/opengoose/pull/41
[pr42]: https://github.com/soilSpoon/opengoose/pull/42
[pr44]: https://github.com/soilSpoon/opengoose/pull/44
[pr46]: https://github.com/soilSpoon/opengoose/pull/46
[pr62]: https://github.com/soilSpoon/opengoose/pull/62
[pr104]: https://github.com/soilSpoon/opengoose/pull/104
[stream-commit]: https://github.com/soilSpoon/opengoose/commit/a339cfbe9e402d543c5c4a447dc9d41a36ce7b2e
[ope105]: https://github.com/soilSpoon/opengoose/commit/e5d5392f42b12b0288e35b08b09ae033459516f5

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
