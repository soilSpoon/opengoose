use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tracing::{debug, info, warn};

use crate::engine::Engine;
use crate::error::GatewayError;
use opengoose_types::{AppEventKind, SessionKey};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::pairing::PairingStore;
use goose::gateway::{Gateway, IncomingMessage, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

/// Prefix of the Goose response that confirms a successful pairing.
const PAIRING_CONFIRMED_PREFIX: &str = "Paired!";

/// Exact Goose response that prompts the user to enter a pairing code.
const PAIRING_PROMPT: &str = "Welcome! Enter your pairing code to connect to goose.";

/// Goose Gateway adapter — thin wrapper around the platform-agnostic Engine.
///
/// Receives events from platform adapters (Discord, etc.), delegates business
/// logic to Engine, and forwards Goose responses back via response_tx.
pub struct OpenGooseGateway {
    response_tx: tokio::sync::mpsc::Sender<(SessionKey, String)>,
    handler: tokio::sync::RwLock<Option<GatewayHandler>>,
    pairing_store: tokio::sync::RwLock<Option<Arc<PairingStore>>>,
    engine: Arc<Engine>,
    /// Platform identifier (e.g. "discord", "slack", "cli").
    platform: String,
}

impl OpenGooseGateway {
    pub fn new(
        response_tx: tokio::sync::mpsc::Sender<(SessionKey, String)>,
        engine: Arc<Engine>,
        platform: impl Into<String>,
    ) -> Self {
        Self {
            response_tx,
            handler: tokio::sync::RwLock::new(None),
            pairing_store: tokio::sync::RwLock::new(None),
            engine,
            platform: platform.into(),
        }
    }

    /// Get a reference to the platform-agnostic engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Store the pairing store reference for later use.
    pub async fn set_pairing_store(&self, store: Arc<PairingStore>) {
        *self.pairing_store.write().await = Some(store);
    }

    /// Generate a 6-character pairing code (300s expiry) and emit it on the event bus.
    pub async fn generate_pairing_code(&self) -> Result<String, GatewayError> {
        let guard = self.pairing_store.read().await;
        let store = guard.as_ref().ok_or(GatewayError::PairingStoreNotReady)?;

        let code = PairingStore::generate_code();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            + 300;

        store
            .store_pending_code(&code, &self.platform, expires_at)
            .await?;

        self.engine
            .event_bus()
            .emit(AppEventKind::PairingCodeGenerated { code: code.clone() });

        Ok(code)
    }

    /// Send a response back through the response channel.
    ///
    /// Awaits the send to apply backpressure when the channel is full,
    /// rather than silently dropping the response.
    pub async fn send_response(&self, session_key: &SessionKey, text: String) {
        if self
            .response_tx
            .send((session_key.clone(), text))
            .await
            .is_err()
        {
            warn!(%session_key, "response channel closed, dropping team response");
        }
    }

    /// Called by platform adapters to relay a message.
    ///
    /// If a team is active for the session, runs team orchestration and sends
    /// the response back. Otherwise falls through to the Goose single-agent handler.
    pub async fn relay_message(
        &self,
        session_key: &SessionKey,
        display_name: Option<String>,
        text: &str,
    ) -> anyhow::Result<()> {
        // Try team orchestration via Engine first
        match self
            .engine
            .process_message(session_key, display_name.as_deref(), text)
            .await?
        {
            Some(response) => {
                // Team handled it — send the response back
                self.send_response(session_key, response).await;
                return Ok(());
            }
            None => {
                // No team active — fall through to Goose single-agent
            }
        }

        let guard = self.handler.read().await;
        let handler = guard.as_ref().ok_or(GatewayError::HandlerNotReady)?;

        let incoming = IncomingMessage {
            user: PlatformUser {
                platform: self.platform.clone(),
                user_id: session_key.to_stable_id(),
                display_name,
            },
            text: text.to_string(),
            platform_message_id: None,
            attachments: vec![],
        };

        handler.handle_message(incoming).await?;
        Ok(())
    }

    /// Get a session store handle (convenience, delegates to engine).
    pub fn sessions(&self) -> opengoose_persistence::SessionStore {
        self.engine.sessions()
    }
}

#[async_trait]
impl Gateway for OpenGooseGateway {
    fn gateway_type(&self) -> &str {
        &self.platform
    }

    async fn start(
        &self,
        handler: GatewayHandler,
        _cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        info!(platform = %self.platform, "opengoose gateway registered with goose");
        self.engine.event_bus().emit(AppEventKind::GooseReady);
        *self.handler.write().await = Some(handler);
        Ok(())
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        if let OutgoingMessage::Text { body } = message {
            let session_key = SessionKey::from_stable_id(&user.user_id);

            // Persist assistant message (from single-agent path)
            self.engine.record_assistant_message(&session_key, &body);

            // Emit PairingCompleted when goose confirms pairing
            if body.starts_with(PAIRING_CONFIRMED_PREFIX) {
                self.engine
                    .event_bus()
                    .emit(AppEventKind::PairingCompleted {
                        session_key: session_key.clone(),
                    });
            }

            // Auto-generate pairing code (shown in TUI only, user enters it in Discord)
            if body == PAIRING_PROMPT
                && let Err(e) = self.generate_pairing_code().await
            {
                info!("failed to auto-generate pairing code: {e}");
            }

            self.engine.event_bus().emit(AppEventKind::ResponseSent {
                session_key: session_key.clone(),
                content: body.clone(),
            });
            if self.response_tx.send((session_key, body)).await.is_err() {
                warn!("response channel closed, dropping message");
            }
        } else {
            debug!("typing indicator for {}", user.user_id);
        }
        Ok(())
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn info(&self) -> HashMap<String, String> {
        HashMap::from([("type".into(), "opengoose".into())])
    }
}
