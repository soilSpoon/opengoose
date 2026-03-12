# OpenGoose v2 Architecture Design

**Status:** Draft
**Last Updated:** 2026-03-12
**Target:** OpenGoose v2.0

## 1. Current State Summary

The OpenGoose architecture follows a layered approach where platform-specific logic is isolated in adapter crates, while business logic and AI orchestration are centralized in `opengoose-core`.

### Dependency Graph
- **opengoose-types**: Fundamental shared types (Platform, SessionKey, AppEvents). No internal dependencies.
- **opengoose-core**: The orchestrator. Depends on `types`, `profiles`, `teams`, and `persistence`.
- **opengoose-adapters** (Discord, Slack, Telegram, Matrix): Transport layers. Depend on `types` and `core`.

### Core Exports
- **Engine**: The primary AI routing logic. Processes incoming messages and drives the streaming response via `AgentRunner`.
- **GatewayBridge**: A bridge between the raw Goose `Gateway` trait and the OpenGoose engine. It provides `relay_and_drive_stream()` to handle orchestration centrally.
- **StreamResponder**: A trait defining the capability to send, edit, and finalize messages for streaming.
- **RelayParams**: A configuration struct for message relaying, including session keys, responders, and throttle policies.

### Adapter Responsibilities
Current adapters (Discord, Slack, Telegram, Matrix) are responsible for:
- **Transport**: Managing WebSockets (Discord/Slack) or Long Polling (Telegram/Matrix).
- **Message Formatting**: Mapping platform-specific markup (Discord Markdown, Slack blocks) to plain text.
- **Reconnection**: Implementing retry logic with exponential backoff.
- **Deduplication**: Filtering out duplicate messages (common in WebSockets and polling).

---

## 2. Problems to Solve

Despite the centralization in `GatewayBridge`, significant boilerplate remains duplicated across all adapters.

### 2.1 Message Deduplication Duplicated
All four adapters implement an independent deduplication mechanism using `HashSet` + `Vec` for LRU eviction.
- **Discord**: `seen: HashSet<Id<MessageMarker>>` + `seen_order: Vec<Id<MessageMarker>>` (mod.rs:121)
- **Telegram**: Often handled via `offset` in polling, but message ID tracking is still required for robustness.
- **Slack**: Handled via `envelope_id` tracking in Socket Mode.

### 2.2 Reconnection Logic Duplicated
Adapters independently implement exponential backoff and max attempt tracking.
- **Telegram**: `MAX_RECONNECT_ATTEMPTS` (36) + `reconnect_delay` (123) in `polling.rs`.
- **Slack**: `MAX_RECONNECT_ATTEMPTS` (26) + `websocket_reconnect_delay` (173) in `socket.rs`.

### 2.3 Event Loop Boilerplate
All adapters implement nearly identical `tokio::select!` patterns for handling shutdown signals (`CancellationToken`) alongside their primary event stream. This leads to redundant code in every `Gateway::start` implementation.

### 2.4 Lack of a Unified Adapter Interface
Adapters currently implement the raw Goose `Gateway` trait. While functional, there is no OpenGoose-specific contract that enforces shared behaviors like structured logging, unified metrics reporting, or standard state transitions.

---

## 3. Proposed Abstractions

### 3.1 `MessageDeduplicator` Utility (Core)
Extract the HashSet+Vec LRU pattern into a reusable utility in `opengoose-core::message_utils`.

```rust
// crates/opengoose-core/src/message_utils/dedup.rs
pub struct MessageDeduplicator<T = String> {
    seen: HashSet<T>,
    order: VecDeque<T>,
    capacity: usize,
}

impl<T: Hash + Eq + Clone> MessageDeduplicator<T> {
    pub fn new(capacity: usize) -> Self;
    pub fn is_seen(&mut self, id: T) -> bool; // Returns true if seen, otherwise inserts
}
```
**Ref:** Replaces logic in `crates/opengoose-discord/src/gateway/mod.rs:139-147`.

### 3.2 `ReconnectPolicy` (Core)
Standardize retry logic in `opengoose-core::throttle` or a new `opengoose-core::retry` module.

```rust
// crates/opengoose-core/src/retry.rs
pub struct ReconnectPolicy {
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
}

impl ReconnectPolicy {
    pub fn new(max_attempts: u32) -> Self;
    pub async fn retry_loop<F, Fut, T, E>(&self, mut connect_fn: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>;
}
```
**Ref:** Replaces `reconnect_delay` in Telegram/Slack adapters.

### 3.3 `ChannelAdapter` Trait (Core)
Define a trait that sits between the transport and `GatewayBridge`, providing a high-level API for OpenGoose.

```rust
// crates/opengoose-core/src/bridge/adapter.rs
#[async_trait]
pub trait ChannelAdapter: StreamResponder {
    fn platform(&self) -> Platform;
    async fn run(&self, cancel: CancellationToken) -> anyhow::Result<()>;
    async fn handle_incoming(&self, event: PlatformEvent) -> anyhow::Result<()>;
}
```

This abstraction allows `opengoose-core` to provide a default event loop implementation:
```rust
pub async fn start_adapter<A: ChannelAdapter>(adapter: Arc<A>, cancel: CancellationToken) {
    // Shared logic for metrics, ready events, and shutdown select!
}
```

---

## 4. Migration Strategy

### Phase 1: Utility Extraction (Non-breaking)
1. Implement `MessageDeduplicator` and `ReconnectPolicy` in `opengoose-core`.
2. Update `opengoose-discord` and `opengoose-telegram` to use these utilities.
3. This reduces LoC in adapters immediately without changing trait signatures.

### Phase 2: `ChannelAdapter` Trait Definition
1. Define the trait in `opengoose-core`.
2. Gradually implement `ChannelAdapter` for existing gateways.
3. Adapters will temporarily implement both `Gateway` (for Goose) and `ChannelAdapter` (for internal structure).

### Phase 3: Boilerplate Reduction
1. Introduce a shared event loop in `opengoose-core` that consumes a `ChannelAdapter`.
2. Move the `tokio::select!` shutdown logic and metrics reporting into the core loop.
3. Dramatically simplify adapter `start()` methods to focus solely on transport initialization.

---

## 5. Adding a New Platform (v2)

With the v2 architecture, adding a platform (e.g., WhatsApp) follows these steps:

1. **Implement `ChannelAdapter`**: Define how to send/edit messages and identify the platform.
2. **Transport Loop**: Write the specific WebSocket or Polling logic.
3. **Use Core Utilities**:
   - Use `MessageDeduplicator` for incoming events.
   - Use `ReconnectPolicy` for connection stability.
   - Use `GatewayBridge::relay_and_drive_stream()` for message processing.

**Expected LoC Reduction:** Current adapters average 300-400 lines for the gateway module. v2 adapters are expected to be <150 lines, focused entirely on the platform's API surface.

---

## 6. Open Questions

- **Event Loop Ownership**: Should core strictly own the loop, or do some platforms (like Matrix with complex `/sync` state) require custom loop logic that makes a trait-based loop too restrictive?
- **Message Formatting**: Should core provide a Markdown-to-Slack-Blocks converter, or keep all formatting in adapters to preserve the "minimal core" principle?
- **Rate Limiting**: Should `ThrottlePolicy` remain per-adapter, or should core manage global rate limits across multiple instances of the same platform?
- **Shared Secrets**: How should the v2 architecture interact with `opengoose-secrets` for managed credential rotation during reconnects?
