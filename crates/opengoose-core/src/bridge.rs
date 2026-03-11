use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing::{debug, info};

use crate::engine::Engine;
use crate::error::GatewayError;
use opengoose_types::{AppEventKind, SessionKey, StreamChunk};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::pairing::PairingStore;
use goose::gateway::{IncomingMessage, PlatformUser};

/// Prefix of the Goose response that confirms a successful pairing.
const PAIRING_CONFIRMED_PREFIX: &str = "Paired!";

/// Exact Goose response that prompts the user to enter a pairing code.
const PAIRING_PROMPT: &str = "Welcome! Enter your pairing code to connect to goose.";

type RateLimitCounters = Arc<Mutex<HashMap<String, Vec<Instant>>>>;

/// Sliding-window limit applied to incoming gateway messages for a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayMessageRateLimit {
    /// Maximum messages accepted within the window.
    pub max_messages: u64,
    /// Sliding window duration.
    pub window: Duration,
}

impl GatewayMessageRateLimit {
    pub fn new(max_messages: u64, window: Duration) -> Self {
        Self {
            max_messages,
            window,
        }
    }
}

impl Default for GatewayMessageRateLimit {
    fn default() -> Self {
        Self::new(20, Duration::from_secs(10))
    }
}

/// Configurable gateway bridge message limits with per-platform overrides.
#[derive(Debug, Clone)]
pub struct GatewayRateLimitConfig {
    default: GatewayMessageRateLimit,
    per_platform: HashMap<String, GatewayMessageRateLimit>,
}

impl GatewayRateLimitConfig {
    pub fn new(default: GatewayMessageRateLimit) -> Self {
        Self {
            default,
            per_platform: HashMap::new(),
        }
    }

    pub fn with_platform_limit(
        mut self,
        platform: impl Into<String>,
        limit: GatewayMessageRateLimit,
    ) -> Self {
        self.per_platform
            .insert(platform.into().to_ascii_lowercase(), limit);
        self
    }

    pub fn limit_for(&self, platform: &str) -> &GatewayMessageRateLimit {
        self.per_platform
            .get(&platform.to_ascii_lowercase())
            .unwrap_or(&self.default)
    }
}

impl Default for GatewayRateLimitConfig {
    fn default() -> Self {
        Self::new(GatewayMessageRateLimit::default())
    }
}

#[derive(Clone)]
struct GatewayMessageRateLimiter {
    config: GatewayRateLimitConfig,
    counters: RateLimitCounters,
}

impl GatewayMessageRateLimiter {
    fn new(config: GatewayRateLimitConfig) -> Self {
        Self {
            config,
            counters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn check(&self, session_key: &SessionKey) -> Result<(), GatewayError> {
        self.check_at(session_key, Instant::now())
    }

    fn check_at(&self, session_key: &SessionKey, now: Instant) -> Result<(), GatewayError> {
        let limit = self.config.limit_for(session_key.platform.as_str());
        if limit.max_messages == 0 {
            return Ok(());
        }

        let bucket_key = format!(
            "{}:{}:{}",
            session_key.to_stable_id(),
            limit.max_messages,
            limit.window.as_millis()
        );
        let mut map = self.counters.lock().unwrap_or_else(|e| e.into_inner());
        let entries = map.entry(bucket_key).or_default();

        entries.retain(|&t| now.duration_since(t) < limit.window);

        if entries.len() as u64 >= limit.max_messages {
            let oldest = entries[0];
            let retry_after_secs = limit
                .window
                .checked_sub(now.duration_since(oldest))
                .unwrap_or(Duration::ZERO)
                .as_secs()
                + 1;
            return Err(GatewayError::RateLimited {
                session_key: session_key.clone(),
                retry_after_secs,
            });
        }

        entries.push(now);
        Ok(())
    }
}

/// Shared orchestration bridge used by all channel gateways.
///
/// Encapsulates the common logic that every opengoose channel gateway needs:
/// - Engine intercept (team orchestration check before Goose single-agent)
/// - GatewayHandler management
/// - Pairing code generation
/// - Persistence and event emission
pub struct GatewayBridge {
    engine: Arc<Engine>,
    handler: tokio::sync::RwLock<Option<GatewayHandler>>,
    pairing_store: tokio::sync::RwLock<Option<Arc<PairingStore>>>,
    message_rate_limiter: GatewayMessageRateLimiter,
}

impl GatewayBridge {
    const SHUTDOWN_MESSAGE: &str = "OpenGoose is shutting down and is not accepting new messages.";

    pub fn new(engine: Arc<Engine>) -> Self {
        Self::with_rate_limit_config(engine, GatewayRateLimitConfig::default())
    }

    pub fn with_rate_limit_config(
        engine: Arc<Engine>,
        rate_limit_config: GatewayRateLimitConfig,
    ) -> Self {
        Self {
            engine,
            handler: tokio::sync::RwLock::new(None),
            pairing_store: tokio::sync::RwLock::new(None),
            message_rate_limiter: GatewayMessageRateLimiter::new(rate_limit_config),
        }
    }

    /// Called by Gateway.start() — stores the handler and emits GooseReady.
    pub async fn on_start(&self, handler: GatewayHandler) {
        info!("gateway_bridge_start: opengoose gateway bridge registered with goose");
        self.engine.event_bus().emit(AppEventKind::GooseReady);
        *self.handler.write().await = Some(handler);
    }

    /// Store the pairing store reference for later use.
    pub async fn set_pairing_store(&self, store: Arc<PairingStore>) {
        *self.pairing_store.write().await = Some(store);
    }

    /// Get a reference to the platform-agnostic engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Get a session store handle (convenience, delegates to engine).
    pub fn sessions(&self) -> &opengoose_persistence::SessionStore {
        self.engine.sessions()
    }

    /// Whether the runtime is still accepting new incoming messages.
    pub fn is_accepting_messages(&self) -> bool {
        self.engine.is_accepting_messages()
    }

    pub fn shutdown_message(&self) -> &'static str {
        Self::SHUTDOWN_MESSAGE
    }

    /// Handle a `/team` or `!team` pairing command and return the response string.
    ///
    /// "Pairing" here means associating a channel with a team/profile so that
    /// subsequent messages in that channel are routed to the selected team.
    ///
    /// Centralizes the pairing dispatch so adapter implementations do not need
    /// to reach into `engine()` directly.  Each adapter still owns the
    /// platform-specific delivery of the returned string.
    ///
    /// # Examples
    /// - `args = "code-review"` — activate the "code-review" team for this channel
    /// - `args = "off"` — deactivate the current team
    /// - `args = ""` — return status of the active team
    /// - `args = "list"` — list available teams
    pub fn handle_pairing(&self, session_key: &SessionKey, args: &str) -> String {
        if !self.is_accepting_messages() {
            return self.shutdown_message().to_string();
        }
        self.engine.handle_team_command(session_key, args)
    }

    /// Generate a 6-character pairing code (300s expiry) and emit it on the event bus.
    pub async fn generate_pairing_code(&self, platform: &str) -> Result<String, GatewayError> {
        debug!(gateway_type = %platform, "generate_pairing_code");

        let guard = self.pairing_store.read().await;
        let store = guard.as_ref().ok_or(GatewayError::PairingStoreNotReady)?;

        let code = PairingStore::generate_code();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            + 300;

        store
            .store_pending_code(&code, platform, expires_at)
            .await?;

        self.engine
            .event_bus()
            .emit(AppEventKind::PairingCodeGenerated { code: code.clone() });

        Ok(code)
    }

    /// Relay an incoming message through the Engine and Goose handler.
    ///
    /// Returns `Some(receiver)` if a team handles the message via streaming.
    /// Returns `None` if no team is active (falls through to Goose single-agent,
    /// which responds via the `Gateway::send_message` callback — no streaming).
    pub async fn relay_message_streaming(
        &self,
        session_key: &SessionKey,
        display_name: Option<String>,
        text: &str,
    ) -> anyhow::Result<Option<tokio::sync::broadcast::Receiver<StreamChunk>>> {
        info!(gateway_type = "bridge", message_type = "streaming", session_id = %session_key.to_stable_id(), "relay_message");

        if !self.is_accepting_messages() {
            return Err(GatewayError::ShuttingDown.into());
        }

        self.check_message_rate_limit(session_key)?;

        // Try streaming team orchestration via Engine
        match self
            .engine
            .process_message_streaming(session_key, display_name.as_deref(), text)
            .await?
        {
            Some(rx) => return Ok(Some(rx)),
            None => {
                // No team active — fall through to Goose single-agent
            }
        }

        let guard = self.handler.read().await;
        let handler = guard.as_ref().ok_or(GatewayError::HandlerNotReady)?;

        let incoming = IncomingMessage {
            user: PlatformUser {
                platform: session_key.platform.as_str().to_string(),
                user_id: session_key.to_stable_id(),
                display_name,
            },
            text: text.to_string(),
            platform_message_id: None,
            attachments: vec![],
        };

        handler.handle_message(incoming).await?;
        Ok(None)
    }

    fn check_message_rate_limit(&self, session_key: &SessionKey) -> Result<(), GatewayError> {
        let result = self.message_rate_limiter.check(session_key);
        if let Err(GatewayError::RateLimited {
            retry_after_secs, ..
        }) = &result
        {
            info!(
                platform = %session_key.platform,
                session_id = %session_key.to_stable_id(),
                retry_after_secs = *retry_after_secs,
                "gateway message rate limited"
            );
        }
        result
    }

    /// Relay an incoming message with streaming, and drive the stream to
    /// completion if a team handles it.
    ///
    /// This combines `relay_message_streaming` + `drive_stream` into a single
    /// call, eliminating the boilerplate duplicated across every channel gateway.
    ///
    /// Returns `true` if a team handled the message (caller should NOT expect
    /// a `send_message` callback), `false` if the Goose single-agent path was
    /// used (response arrives via `Gateway::send_message`).
    #[allow(clippy::too_many_arguments)]
    pub async fn relay_and_drive_stream(
        &self,
        session_key: &SessionKey,
        display_name: Option<String>,
        text: &str,
        responder: &dyn crate::StreamResponder,
        channel_id: &str,
        throttle: crate::ThrottlePolicy,
        max_display_len: usize,
    ) -> anyhow::Result<bool> {
        let result = self
            .relay_and_drive_stream_inner(
                session_key,
                display_name,
                text,
                responder,
                channel_id,
                throttle,
                max_display_len,
            )
            .await;

        // Emit error event centrally so adapters don't have to repeat this
        if let Err(ref e) = result {
            self.engine.event_bus().emit(AppEventKind::Error {
                context: "relay".into(),
                message: e.to_string(),
            });
        }

        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn relay_and_drive_stream_inner(
        &self,
        session_key: &SessionKey,
        display_name: Option<String>,
        text: &str,
        responder: &dyn crate::StreamResponder,
        channel_id: &str,
        throttle: crate::ThrottlePolicy,
        max_display_len: usize,
    ) -> anyhow::Result<bool> {
        info!(gateway_type = "bridge", message_type = "streaming", session_id = %session_key.to_stable_id(), channel_id = %channel_id, "relay_and_drive_stream");

        match self
            .relay_message_streaming(session_key, display_name, text)
            .await?
        {
            Some(rx) => {
                crate::stream_orchestrator::drive_stream(
                    responder,
                    channel_id,
                    rx,
                    throttle,
                    max_display_len,
                )
                .await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Called from `Gateway::send_message` — handles persistence, pairing detection,
    /// and event emission for outgoing messages from the Goose single-agent path.
    ///
    /// Returns the decoded `SessionKey` so the bridge can route replies back to
    /// the originating channel without adapters re-parsing the stable ID.
    async fn on_outgoing_message(
        &self,
        user_id: &str,
        body: &str,
        gateway_type: &str,
    ) -> SessionKey {
        info!(gateway_type = %gateway_type, message_type = "response", "outgoing_message");
        let session_key = SessionKey::from_stable_id(user_id);

        // Persist assistant message (from single-agent path)
        self.engine.record_assistant_message(&session_key, body);

        // Emit PairingCompleted when goose confirms pairing
        if body.starts_with(PAIRING_CONFIRMED_PREFIX) {
            self.engine
                .event_bus()
                .emit(AppEventKind::PairingCompleted {
                    session_key: session_key.clone(),
                });
        }

        // Auto-generate pairing code
        if body == PAIRING_PROMPT
            && let Err(e) = self.generate_pairing_code(gateway_type).await
        {
            info!("failed to auto-generate pairing code: {e}");
        }

        self.engine.event_bus().emit(AppEventKind::ResponseSent {
            session_key: session_key.clone(),
            content: body.to_string(),
        });

        session_key
    }

    /// Persist an outgoing Goose response, emit pairing events when needed, and
    /// return the destination channel ID for platform-specific delivery.
    pub async fn route_outgoing_text(
        &self,
        user_id: &str,
        body: &str,
        gateway_type: &str,
    ) -> String {
        self.on_outgoing_message(user_id, body, gateway_type)
            .await
            .channel_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::OnceLock;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use goose::config::Config;
    use opengoose_persistence::Database;
    use opengoose_types::{EventBus, Platform};
    use tokio::sync::broadcast::error::TryRecvError;
    use uuid::Uuid;

    use crate::{DraftHandle, ThrottlePolicy};

    static GOOSE_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    static GOOSE_PATH_ROOT: OnceLock<std::path::PathBuf> = OnceLock::new();

    fn ensure_goose_test_root() {
        let root = GOOSE_PATH_ROOT.get_or_init(|| {
            let root = std::env::temp_dir().join(format!("opengoose-goose-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            // Safety: tests serialize access with GOOSE_ENV_LOCK before any Goose config is initialized.
            unsafe {
                std::env::set_var("GOOSE_PATH_ROOT", &root);
                std::env::set_var("GOOSE_DISABLE_KEYRING", "1");
            }
            root
        });

        std::fs::create_dir_all(root).unwrap();
        let config = Config::global();
        if config.exists() {
            let _ = config.clear();
        }
    }

    fn test_key() -> SessionKey {
        SessionKey::direct(Platform::Discord, "user-1")
    }

    fn test_engine(event_bus: EventBus) -> Arc<Engine> {
        Arc::new(Engine::new_with_team_store(
            event_bus,
            Database::open_in_memory().unwrap(),
            None,
        ))
    }

    #[derive(Default)]
    struct FailingCreateResponder;

    #[async_trait::async_trait]
    impl crate::StreamResponder for FailingCreateResponder {
        fn supports_streaming(&self) -> bool {
            true
        }

        fn max_message_len(&self) -> usize {
            2_000
        }

        async fn create_draft(&self, _channel_id: &str) -> anyhow::Result<DraftHandle> {
            Err(anyhow::anyhow!("draft creation failed"))
        }

        async fn update_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn send_new_message(&self, _channel_id: &str, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingResponder {
        drafts: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl crate::StreamResponder for RecordingResponder {
        fn supports_streaming(&self) -> bool {
            true
        }

        fn max_message_len(&self) -> usize {
            2_000
        }

        async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
            self.drafts.lock().unwrap().push(channel_id.to_string());
            Ok(DraftHandle {
                message_id: "draft-1".into(),
                channel_id: channel_id.into(),
            })
        }

        async fn update_draft(&self, _handle: &DraftHandle, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn send_new_message(&self, _channel_id: &str, _content: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn gateway_rate_limit_config_uses_platform_override() {
        let config = GatewayRateLimitConfig::default().with_platform_limit(
            "slack",
            GatewayMessageRateLimit::new(3, Duration::from_secs(30)),
        );

        assert_eq!(
            config.limit_for("slack"),
            &GatewayMessageRateLimit::new(3, Duration::from_secs(30))
        );
        assert_eq!(
            config.limit_for("discord"),
            &GatewayMessageRateLimit::default()
        );
    }

    #[test]
    fn gateway_message_rate_limiter_is_scoped_per_session() {
        let limiter = GatewayMessageRateLimiter::new(GatewayRateLimitConfig::new(
            GatewayMessageRateLimit::new(1, Duration::from_secs(60)),
        ));
        let now = Instant::now();
        let first = SessionKey::direct(Platform::Discord, "chan-1");
        let second = SessionKey::direct(Platform::Discord, "chan-2");

        assert!(limiter.check_at(&first, now).is_ok());
        assert!(matches!(
            limiter.check_at(&first, now + Duration::from_secs(1)),
            Err(GatewayError::RateLimited { .. })
        ));
        assert!(
            limiter
                .check_at(&second, now + Duration::from_secs(1))
                .is_ok()
        );
    }

    #[test]
    fn gateway_message_rate_limiter_honors_platform_specific_limits() {
        let limiter = GatewayMessageRateLimiter::new(
            GatewayRateLimitConfig::new(GatewayMessageRateLimit::new(3, Duration::from_secs(60)))
                .with_platform_limit(
                    "telegram",
                    GatewayMessageRateLimit::new(1, Duration::from_secs(60)),
                ),
        );
        let now = Instant::now();
        let telegram = SessionKey::direct(Platform::Telegram, "chat-1");
        let discord = SessionKey::direct(Platform::Discord, "chat-1");

        assert!(limiter.check_at(&telegram, now).is_ok());
        assert!(matches!(
            limiter.check_at(&telegram, now + Duration::from_secs(1)),
            Err(GatewayError::RateLimited { .. })
        ));

        assert!(limiter.check_at(&discord, now).is_ok());
        assert!(
            limiter
                .check_at(&discord, now + Duration::from_secs(1))
                .is_ok()
        );
        assert!(
            limiter
                .check_at(&discord, now + Duration::from_secs(2))
                .is_ok()
        );
        assert!(matches!(
            limiter.check_at(&discord, now + Duration::from_secs(3)),
            Err(GatewayError::RateLimited { .. })
        ));
    }

    #[test]
    fn gateway_message_rate_limiter_zero_limit_disables_limiting() {
        let limiter = GatewayMessageRateLimiter::new(GatewayRateLimitConfig::new(
            GatewayMessageRateLimit::new(0, Duration::from_secs(60)),
        ));
        let key = SessionKey::direct(Platform::Discord, "chan-1");
        let now = Instant::now();

        for offset in 0..5 {
            assert!(
                limiter
                    .check_at(&key, now + Duration::from_secs(offset))
                    .is_ok()
            );
        }
    }

    #[test]
    fn gateway_message_rate_limiter_recovers_after_window_expires() {
        let limiter = GatewayMessageRateLimiter::new(GatewayRateLimitConfig::new(
            GatewayMessageRateLimit::new(1, Duration::from_secs(10)),
        ));
        let key = SessionKey::direct(Platform::Discord, "chan-1");
        let now = Instant::now();

        assert!(limiter.check_at(&key, now).is_ok());
        assert!(matches!(
            limiter.check_at(&key, now + Duration::from_secs(5)),
            Err(GatewayError::RateLimited { .. })
        ));
        assert!(
            limiter
                .check_at(&key, now + Duration::from_secs(11))
                .is_ok()
        );
    }

    #[test]
    fn gateway_message_rate_limiter_retry_after_rounds_up_to_next_second() {
        let limiter = GatewayMessageRateLimiter::new(GatewayRateLimitConfig::new(
            GatewayMessageRateLimit::new(1, Duration::from_millis(1_500)),
        ));
        let key = SessionKey::direct(Platform::Discord, "chan-1");
        let now = Instant::now();

        assert!(limiter.check_at(&key, now).is_ok());
        match limiter.check_at(&key, now + Duration::from_millis(1)) {
            Err(GatewayError::RateLimited {
                retry_after_secs, ..
            }) => assert_eq!(retry_after_secs, 2),
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn generate_pairing_code_requires_store() {
        let event_bus = EventBus::new(16);
        let bridge = GatewayBridge::new(test_engine(event_bus));

        let err = bridge.generate_pairing_code("discord").await.unwrap_err();

        assert!(matches!(err, GatewayError::PairingStoreNotReady));
    }

    #[tokio::test]
    async fn generate_pairing_code_persists_and_emits_event() {
        let _guard = GOOSE_ENV_LOCK.lock().await;
        ensure_goose_test_root();

        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let store = Arc::new(PairingStore::new().unwrap());
        bridge.set_pairing_store(store.clone()).await;

        let code = bridge.generate_pairing_code("discord").await.unwrap();

        assert_eq!(code.len(), 6);
        assert_eq!(
            store.consume_pending_code(&code).await.unwrap(),
            Some("discord".into())
        );
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::PairingCodeGenerated { code: emitted } if emitted == code
        ));
    }

    #[tokio::test]
    async fn outgoing_message_persists_history_and_pairing_events() {
        let _guard = GOOSE_ENV_LOCK.lock().await;
        ensure_goose_test_root();

        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key = test_key();
        let body = "Paired! You can now chat with goose.";

        bridge
            .on_outgoing_message(&key.to_stable_id(), body, "discord")
            .await;

        let history = bridge.sessions().load_history(&key, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "assistant");
        assert_eq!(history[0].content, body);
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::PairingCompleted { session_key } if session_key == key
        ));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::ResponseSent { session_key, content } if session_key == key && content == body
        ));
    }

    #[tokio::test]
    async fn outgoing_pairing_prompt_auto_generates_code() {
        let _guard = GOOSE_ENV_LOCK.lock().await;
        ensure_goose_test_root();

        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let store = Arc::new(PairingStore::new().unwrap());
        bridge.set_pairing_store(store.clone()).await;
        let key = test_key();

        bridge
            .on_outgoing_message(&key.to_stable_id(), PAIRING_PROMPT, "discord")
            .await;

        let pairing = rx.try_recv().unwrap();
        let code = match pairing.kind {
            AppEventKind::PairingCodeGenerated { code } => code,
            other => unreachable!("expected pairing code event, got {}", other),
        };
        assert_eq!(
            store.consume_pending_code(&code).await.unwrap(),
            Some("discord".into())
        );
        let response = rx.try_recv().unwrap();
        assert!(matches!(
            response.kind,
            AppEventKind::ResponseSent { session_key, content }
            if session_key == key && content == PAIRING_PROMPT
        ));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn route_outgoing_text_returns_channel_id_and_emits_pairing_events() {
        let _guard = GOOSE_ENV_LOCK.lock().await;
        ensure_goose_test_root();

        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let store = Arc::new(PairingStore::new().unwrap());
        bridge.set_pairing_store(store.clone()).await;
        let key = test_key();

        let channel_id = bridge
            .route_outgoing_text(&key.to_stable_id(), PAIRING_PROMPT, "discord")
            .await;

        assert_eq!(channel_id, key.channel_id);
        let pairing = rx.try_recv().unwrap();
        let code = match pairing.kind {
            AppEventKind::PairingCodeGenerated { code } => code,
            other => unreachable!("expected pairing code event, got {}", other),
        };
        assert_eq!(
            store.consume_pending_code(&code).await.unwrap(),
            Some("discord".into())
        );
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::ResponseSent { session_key, content }
            if session_key == key && content == PAIRING_PROMPT
        ));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

        let channel_id = bridge
            .route_outgoing_text(
                &key.to_stable_id(),
                "Paired! You can now chat with goose.",
                "discord",
            )
            .await;

        assert_eq!(channel_id, key.channel_id);
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::PairingCompleted { session_key } if session_key == key
        ));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::ResponseSent { session_key, content }
            if session_key == key && content == "Paired! You can now chat with goose."
        ));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn relay_message_streaming_rejects_messages_over_rate_limit() {
        let bridge = GatewayBridge::with_rate_limit_config(
            test_engine(EventBus::new(16)),
            GatewayRateLimitConfig::new(GatewayMessageRateLimit::new(1, Duration::from_secs(60))),
        );
        let key = test_key();

        let result = bridge
            .relay_message_streaming(&key, Some("alice".into()), "first")
            .await
            .unwrap();
        assert!(result.is_some());

        let err = bridge
            .relay_message_streaming(&key, Some("alice".into()), "second")
            .await
            .unwrap_err();
        match err.downcast::<GatewayError>().unwrap() {
            GatewayError::RateLimited {
                session_key,
                retry_after_secs,
            } => {
                assert_eq!(session_key, key);
                assert!(retry_after_secs > 0);
            }
            other => panic!("expected RateLimited, got {other}"),
        }
    }

    #[tokio::test]
    async fn relay_message_streaming_emits_message_received_then_stream_started() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key = test_key();

        let stream = bridge
            .relay_message_streaming(&key, Some("alice".into()), "hello")
            .await
            .unwrap();

        assert!(stream.is_some());
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::MessageReceived {
                session_key,
                author,
                content,
            } if session_key == key && author == "alice" && content == "hello"
        ));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::StreamStarted { session_key, .. } if session_key == key
        ));
    }

    #[tokio::test]
    async fn relay_message_streaming_persists_user_message_history() {
        let event_bus = EventBus::new(16);
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key = test_key();

        let _ = bridge
            .relay_message_streaming(&key, Some("alice".into()), "stored by bridge")
            .await
            .unwrap();

        let history = bridge.sessions().load_history(&key, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "stored by bridge");
        assert_eq!(history[0].author.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn relay_message_streaming_uses_unknown_author_when_display_name_is_missing() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key = test_key();

        let _ = bridge
            .relay_message_streaming(&key, None, "anonymous")
            .await
            .unwrap();

        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::MessageReceived {
                session_key,
                author,
                content,
            } if session_key == key && author == "unknown" && content == "anonymous"
        ));
    }

    #[tokio::test]
    async fn relay_message_streaming_keeps_sessions_isolated() {
        let event_bus = EventBus::new(16);
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key_a = SessionKey::new(Platform::Discord, "guild-1", "chan-a");
        let key_b = SessionKey::new(Platform::Discord, "guild-1", "chan-b");

        let (result_a, result_b) = tokio::join!(
            bridge.relay_message_streaming(&key_a, Some("alice".into()), "message for A"),
            bridge.relay_message_streaming(&key_b, Some("bob".into()), "message for B"),
        );

        assert!(result_a.unwrap().is_some());
        assert!(result_b.unwrap().is_some());

        let history_a = bridge.sessions().load_history(&key_a, 10).unwrap();
        let history_b = bridge.sessions().load_history(&key_b, 10).unwrap();
        assert_eq!(history_a.len(), 1);
        assert_eq!(history_a[0].content, "message for A");
        assert_eq!(history_b.len(), 1);
        assert_eq!(history_b[0].content, "message for B");
    }

    #[tokio::test]
    async fn relay_message_streaming_rejects_messages_after_shutdown() {
        let event_bus = EventBus::new(16);
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key = test_key();

        bridge.engine().shutdown().await;

        let err = bridge
            .relay_message_streaming(&key, Some("alice".into()), "after shutdown")
            .await
            .unwrap_err();

        assert!(
            err.downcast_ref::<GatewayError>()
                .is_some_and(|err| matches!(err, GatewayError::ShuttingDown))
        );
    }

    #[tokio::test]
    async fn relay_and_drive_stream_emits_error_event_when_responder_fails() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key = test_key();

        let err = bridge
            .relay_and_drive_stream(
                &key,
                Some("alice".into()),
                "hello",
                &FailingCreateResponder,
                "channel-1",
                ThrottlePolicy::discord(),
                2_000,
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("draft creation failed"));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::MessageReceived { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::StreamStarted { session_key, .. } if session_key == key
        ));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::Error { context, message }
            if context == "relay" && message.contains("draft creation failed")
        ));
    }

    #[tokio::test]
    async fn relay_and_drive_stream_emits_error_event_for_shutdown_rejection() {
        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key = test_key();

        bridge.engine().shutdown().await;

        let err = bridge
            .relay_and_drive_stream(
                &key,
                Some("alice".into()),
                "hello",
                &RecordingResponder::default(),
                "channel-1",
                ThrottlePolicy::discord(),
                2_000,
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("shutdown in progress"));
        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::Error { context, message }
            if context == "relay" && message.contains("shutdown in progress")
        ));
    }

    // ── handle_pairing (centralized team/profile routing) ────────────────────

    fn test_engine_with_teams() -> Arc<Engine> {
        use opengoose_teams::TeamStore;
        use uuid::Uuid;
        let event_bus = EventBus::new(16);
        let dir =
            std::env::temp_dir().join(format!("opengoose-bridge-team-store-{}", Uuid::new_v4()));
        let store = TeamStore::with_dir(dir);
        store.install_defaults(false).unwrap();
        Arc::new(Engine::new_with_team_store(
            event_bus,
            Database::open_in_memory().unwrap(),
            Some(store),
        ))
    }

    #[test]
    fn bridge_pairing_no_active_team() {
        let bridge = GatewayBridge::new(test_engine(EventBus::new(16)));
        let key = test_key();
        assert_eq!(
            bridge.handle_pairing(&key, ""),
            "No team active for this channel."
        );
    }

    #[test]
    fn bridge_pairing_list_delegates_to_engine() {
        let bridge = GatewayBridge::new(test_engine_with_teams());
        let key = test_key();
        let response = bridge.handle_pairing(&key, "list");
        assert!(response.starts_with("Available teams:"), "{response}");
        assert!(response.contains("code-review"), "{response}");
    }

    #[test]
    fn bridge_pairing_activate_and_deactivate() {
        let bridge = GatewayBridge::new(test_engine_with_teams());
        let key = test_key();

        let activate = bridge.handle_pairing(&key, "code-review");
        assert_eq!(activate, "Team code-review activated for this channel.");

        let status = bridge.handle_pairing(&key, "");
        assert_eq!(status, "Active team: code-review");

        let deactivate = bridge.handle_pairing(&key, "off");
        assert_eq!(
            deactivate,
            "Team deactivated. Reverting to single-agent mode."
        );

        let empty = bridge.handle_pairing(&key, "");
        assert_eq!(empty, "No team active for this channel.");
    }

    #[test]
    fn bridge_pairing_unknown_team_reports_available() {
        let bridge = GatewayBridge::new(test_engine_with_teams());
        let key = test_key();
        let response = bridge.handle_pairing(&key, "nonexistent");
        assert!(
            response.contains("not found"),
            "expected 'not found' in: {response}"
        );
    }

    #[tokio::test]
    async fn bridge_pairing_returns_shutdown_message_after_shutdown() {
        let bridge = GatewayBridge::new(test_engine_with_teams());
        let key = test_key();

        bridge.engine().shutdown().await;

        assert_eq!(
            bridge.handle_pairing(&key, "list"),
            bridge.shutdown_message()
        );
    }

    #[test]
    fn bridge_pairing_team_selection_is_scoped_per_session() {
        let bridge = GatewayBridge::new(test_engine_with_teams());
        let first = SessionKey::new(Platform::Discord, "guild-1", "chan-a");
        let second = SessionKey::new(Platform::Discord, "guild-1", "chan-b");

        assert_eq!(
            bridge.handle_pairing(&first, "code-review"),
            "Team code-review activated for this channel."
        );
        assert_eq!(
            bridge.handle_pairing(&first, ""),
            "Active team: code-review"
        );
        assert_eq!(
            bridge.handle_pairing(&second, ""),
            "No team active for this channel."
        );
        assert_eq!(
            bridge.handle_pairing(&second, "bug-triage"),
            "Team bug-triage activated for this channel."
        );
        assert_eq!(
            bridge.handle_pairing(&second, ""),
            "Active team: bug-triage"
        );
        assert_eq!(
            bridge.handle_pairing(&first, ""),
            "Active team: code-review"
        );
    }

    #[tokio::test]
    async fn outgoing_non_pairing_message_only_emits_response_sent() {
        let _guard = GOOSE_ENV_LOCK.lock().await;
        ensure_goose_test_root();

        let event_bus = EventBus::new(16);
        let mut rx = event_bus.subscribe();
        let bridge = GatewayBridge::new(test_engine(event_bus));
        let key = test_key();

        bridge
            .on_outgoing_message(&key.to_stable_id(), "plain response", "discord")
            .await;

        assert!(matches!(
            rx.try_recv().unwrap().kind,
            AppEventKind::ResponseSent { session_key, content }
            if session_key == key && content == "plain response"
        ));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }
}
