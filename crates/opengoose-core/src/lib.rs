mod error;
mod gateway;

pub use error::GatewayError;
pub use gateway::OpenGooseGateway;

use std::sync::Arc;

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::info;

use goose::execution::manager::AgentManager;
use goose::gateway::handler::GatewayHandler;
use goose::gateway::pairing::PairingStore;
use goose::gateway::{Gateway, GatewayConfig};

/// Initialize Goose agent system and wire up the gateway.
/// Uses Goose's default config paths (~/.config/goose/).
pub async fn start_gateway(
    gateway: Arc<OpenGooseGateway>,
    cancel: CancellationToken,
) -> Result<()> {
    let agent_manager = AgentManager::instance().await?;
    let pairing_store = Arc::new(PairingStore::new()?);

    // Retain a reference for pairing code generation
    gateway.set_pairing_store(pairing_store.clone()).await;

    let config = GatewayConfig {
        gateway_type: "discord".to_string(),
        platform_config: serde_json::json!({}),
        max_sessions: 100,
    };

    let handler = GatewayHandler::new(
        agent_manager,
        pairing_store,
        gateway.clone() as Arc<dyn Gateway>,
        config,
    );

    info!("starting goose agent system");
    gateway.start(handler, cancel).await?;

    Ok(())
}
