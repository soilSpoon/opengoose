mod outgoing;
mod pairing;
mod relay;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use tracing::{info, instrument};

use crate::engine::Engine;
use opengoose_types::AppEventKind;

use goose::gateway::handler::GatewayHandler;
use goose::gateway::pairing::PairingStore;

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
    const SHUTDOWN_MESSAGE: &str = "OpenGoose is shutting down and is not accepting new messages.";

    pub fn new(engine: Arc<Engine>) -> Self {
        Self {
            engine,
            handler: tokio::sync::RwLock::new(None),
            pairing_store: tokio::sync::RwLock::new(None),
        }
    }

    /// Called by Gateway.start() — stores the handler and emits GooseReady.
    #[instrument(skip(self, handler))]
    pub async fn on_start(&self, handler: GatewayHandler) {
        info!("gateway_bridge_start: opengoose gateway bridge registered with goose");
        self.engine.event_bus().emit(AppEventKind::GooseReady);
        *self.handler.write().await = Some(handler);
    }

    /// Store the pairing store reference for later use.
    #[instrument(skip(self, store))]
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
}
