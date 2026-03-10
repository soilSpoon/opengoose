use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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
}

impl GatewayBridge {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self {
            engine,
            handler: tokio::sync::RwLock::new(None),
            pairing_store: tokio::sync::RwLock::new(None),
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

    /// Handle a `/team` or `!team` command and return the response string.
    ///
    /// Centralizes the team command dispatch so adapter implementations do not
    /// need to reach into `engine()` directly.  Each adapter still owns the
    /// platform-specific delivery of the returned string.
    pub fn handle_team_command(&self, session_key: &SessionKey, args: &str) -> String {
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
    /// Returns the decoded `SessionKey` so callers can reuse it for platform-specific
    /// sending without re-parsing the stable ID.
    pub async fn on_outgoing_message(
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::OnceLock;

    use goose::config::Config;
    use opengoose_persistence::Database;
    use opengoose_types::{EventBus, Platform};
    use tokio::sync::broadcast::error::TryRecvError;
    use uuid::Uuid;

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

    // ── handle_team_command (centralized routing) ────────────────────────────

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
    fn bridge_team_command_no_active_team() {
        let bridge = GatewayBridge::new(test_engine(EventBus::new(16)));
        let key = test_key();
        assert_eq!(
            bridge.handle_team_command(&key, ""),
            "No team active for this channel."
        );
    }

    #[test]
    fn bridge_team_command_list_delegates_to_engine() {
        let bridge = GatewayBridge::new(test_engine_with_teams());
        let key = test_key();
        let response = bridge.handle_team_command(&key, "list");
        assert!(response.starts_with("Available teams:"), "{response}");
        assert!(response.contains("code-review"), "{response}");
    }

    #[test]
    fn bridge_team_command_activate_and_deactivate() {
        let bridge = GatewayBridge::new(test_engine_with_teams());
        let key = test_key();

        let activate = bridge.handle_team_command(&key, "code-review");
        assert_eq!(activate, "Team code-review activated for this channel.");

        let status = bridge.handle_team_command(&key, "");
        assert_eq!(status, "Active team: code-review");

        let deactivate = bridge.handle_team_command(&key, "off");
        assert_eq!(
            deactivate,
            "Team deactivated. Reverting to single-agent mode."
        );

        let empty = bridge.handle_team_command(&key, "");
        assert_eq!(empty, "No team active for this channel.");
    }

    #[test]
    fn bridge_team_command_unknown_team_reports_available() {
        let bridge = GatewayBridge::new(test_engine_with_teams());
        let key = test_key();
        let response = bridge.handle_team_command(&key, "nonexistent");
        assert!(
            response.contains("not found"),
            "expected 'not found' in: {response}"
        );
    }
}
