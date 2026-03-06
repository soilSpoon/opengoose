use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::info;

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
        info!("opengoose gateway bridge registered with goose");
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

    /// Generate a 6-character pairing code (300s expiry) and emit it on the event bus.
    pub async fn generate_pairing_code(&self, platform: &str) -> Result<String, GatewayError> {
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

        if let Err(ref e) = result {
            self.engine.event_bus().emit(AppEventKind::Error {
                context: "relay".into(),
                message: e.to_string(),
            });
            tracing::error!(%e, "failed to relay message to goose");
        }

        result
    }

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
    /// Returns the body text (or None for non-text messages like typing indicators).
    pub async fn on_outgoing_message(&self, user_id: &str, body: &str, gateway_type: &str) {
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
            session_key,
            content: body.to_string(),
        });
    }
}
