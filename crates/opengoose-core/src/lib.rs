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
///
/// Agent profiles are installed to `~/.opengoose/profiles/` and registered
/// in `GOOSE_RECIPE_PATH` so Summon can discover them as sub-recipes.
pub async fn start_gateway(
    gateway: Arc<OpenGooseGateway>,
    cancel: CancellationToken,
) -> Result<()> {
    // Register agent profiles *before* AgentManager init (Summon caches paths at startup)
    let profile_store = opengoose_profiles::ProfileStore::new()?;
    let installed = profile_store.install_defaults(false)?;
    if installed > 0 {
        info!(count = installed, "installed default agent profiles");
    }
    opengoose_profiles::register_profiles_path(profile_store.dir())?;

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
