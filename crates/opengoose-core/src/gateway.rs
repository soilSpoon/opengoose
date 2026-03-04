use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tracing::{debug, info, warn};

use crate::error::GatewayError;
use opengoose_types::{AppEventKind, EventBus, SessionKey};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::pairing::PairingStore;
use goose::gateway::{Gateway, IncomingMessage, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

/// Goose Gateway trait implementation.
/// Receives events from the Discord adapter, relays them to Goose,
/// and forwards Goose responses back via response_tx.
pub struct OpenGooseGateway {
    response_tx: tokio::sync::mpsc::Sender<(SessionKey, String)>,
    handler: tokio::sync::RwLock<Option<GatewayHandler>>,
    pairing_store: tokio::sync::RwLock<Option<Arc<PairingStore>>>,
    event_bus: EventBus,
}

impl OpenGooseGateway {
    pub fn new(
        response_tx: tokio::sync::mpsc::Sender<(SessionKey, String)>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            response_tx,
            handler: tokio::sync::RwLock::new(None),
            pairing_store: tokio::sync::RwLock::new(None),
            event_bus,
        }
    }

    /// Store the pairing store reference for later use.
    pub async fn set_pairing_store(&self, store: Arc<PairingStore>) {
        *self.pairing_store.write().await = Some(store);
    }

    /// Generate a 6-character pairing code (300s expiry) and emit it on the event bus.
    pub async fn generate_pairing_code(&self) -> anyhow::Result<String> {
        let guard = self.pairing_store.read().await;
        let store = guard
            .as_ref()
            .ok_or(GatewayError::PairingStoreNotReady)?;

        let code = PairingStore::generate_code();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 300;

        store
            .store_pending_code(&code, "discord", expires_at)
            .await?;

        self.event_bus
            .emit(AppEventKind::PairingCodeGenerated { code: code.clone() });

        Ok(code)
    }

    /// Called by the Discord adapter to relay a message to Goose.
    pub async fn relay_message(
        &self,
        session_key: &SessionKey,
        display_name: Option<String>,
        text: &str,
    ) -> anyhow::Result<()> {
        let guard = self.handler.read().await;
        let handler = guard
            .as_ref()
            .ok_or(GatewayError::HandlerNotReady)?;

        self.event_bus.emit(AppEventKind::MessageReceived {
            session_key: session_key.clone(),
            author: display_name.clone().unwrap_or_else(|| "unknown".into()),
            content: text.to_string(),
        });

        let incoming = IncomingMessage {
            user: PlatformUser {
                platform: "discord".into(),
                user_id: session_key.to_platform_user_id(),
                display_name,
            },
            text: text.to_string(),
            platform_message_id: None,
            attachments: vec![],
        };

        handler.handle_message(incoming).await?;
        Ok(())
    }
}

#[async_trait]
impl Gateway for OpenGooseGateway {
    fn gateway_type(&self) -> &str {
        "discord"
    }

    async fn start(
        &self,
        handler: GatewayHandler,
        _cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        info!("opengoose gateway registered with goose");
        self.event_bus.emit(AppEventKind::DiscordReady);
        *self.handler.write().await = Some(handler);
        Ok(())
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        if let OutgoingMessage::Text { body } = message {
            let session_key = SessionKey::from_platform_user_id(&user.user_id);
            self.event_bus.emit(AppEventKind::ResponseSent {
                session_key: session_key.clone(),
                content: body.clone(),
            });
            if self.response_tx.send((session_key.clone(), body.clone())).await.is_err() {
                warn!(%session_key, "response channel closed, dropping message");
            }

            // Emit PairingCompleted when goose confirms pairing
            if body.starts_with("Paired!") {
                self.event_bus.emit(AppEventKind::PairingCompleted {
                    session_key: session_key.clone(),
                });
            }

            // Auto-generate pairing code (shown in TUI only, user enters it in Discord)
            if body == "Welcome! Enter your pairing code to connect to goose." {
                if let Err(e) = self.generate_pairing_code().await {
                    info!("failed to auto-generate pairing code: {e}");
                }
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

