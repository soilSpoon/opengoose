use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::info;

use crate::engine::Engine;
use crate::error::GatewayError;
use opengoose_types::{AppEventKind, SessionKey};

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
    pub fn sessions(&self) -> opengoose_persistence::SessionStore {
        self.engine.sessions()
    }

    /// Generate a 6-character pairing code (300s expiry) and emit it on the event bus.
    pub async fn generate_pairing_code(
        &self,
        platform: &str,
    ) -> Result<String, GatewayError> {
        let guard = self.pairing_store.read().await;
        let store = guard.as_ref().ok_or(GatewayError::PairingStoreNotReady)?;

        let code = PairingStore::generate_code();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            + 300;

        store.store_pending_code(&code, platform, expires_at).await?;

        self.engine
            .event_bus()
            .emit(AppEventKind::PairingCodeGenerated { code: code.clone() });

        Ok(code)
    }

    /// Called by incoming messages from any channel.
    ///
    /// Checks Engine first for team orchestration. If a team handles the message,
    /// returns `Some(response)`. Otherwise falls through to the Goose single-agent
    /// handler (which will respond via the `Gateway::send_message` callback) and
    /// returns `None`.
    pub async fn relay_message(
        &self,
        session_key: &SessionKey,
        display_name: Option<String>,
        text: &str,
    ) -> anyhow::Result<Option<String>> {
        // Try team orchestration via Engine first
        match self
            .engine
            .process_message(session_key, display_name.as_deref(), text)
            .await?
        {
            Some(response) => {
                // Team handled it — caller should send response directly
                return Ok(Some(response));
            }
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

    /// Called from `Gateway::send_message` — handles persistence, pairing detection,
    /// and event emission for outgoing messages from the Goose single-agent path.
    ///
    /// Returns the body text (or None for non-text messages like typing indicators).
    pub async fn on_outgoing_message(
        &self,
        user_id: &str,
        body: &str,
        gateway_type: &str,
    ) {
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
