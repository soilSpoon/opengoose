mod engine;
mod error;
mod gateway;

pub use engine::Engine;
pub use error::GatewayError;
pub use gateway::OpenGooseGateway;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::info;

use goose::execution::manager::AgentManager;
use goose::gateway::handler::GatewayHandler;
use goose::gateway::pairing::PairingStore;
use goose::gateway::{Gateway, GatewayConfig};

/// Install default profiles and teams, and register the profiles path in
/// `GOOSE_RECIPE_PATH`.
///
/// **Must be called before the tokio multi-thread runtime is started** because
/// it uses `unsafe { std::env::set_var }` internally.
pub fn setup_profiles_and_teams() -> Result<(), GatewayError> {
    let profile_store = opengoose_profiles::ProfileStore::new()?;
    let installed = profile_store.install_defaults(false)?;
    if installed > 0 {
        info!(count = installed, "installed default agent profiles");
    }
    opengoose_profiles::register_profiles_path(profile_store.dir())?;

    let team_store = opengoose_teams::TeamStore::new()?;
    let teams_installed = team_store.install_defaults(false)?;
    if teams_installed > 0 {
        info!(count = teams_installed, "installed default team definitions");
    }

    Ok(())
}

/// Initialize Goose agent system and wire up the gateway.
/// Uses Goose's default config paths (~/.config/goose/).
///
/// Assumes `setup_profiles_and_teams()` has already been called.
pub async fn start_gateway(
    gateway: Arc<OpenGooseGateway>,
    cancel: CancellationToken,
) -> Result<(), GatewayError> {
    let agent_manager = AgentManager::instance().await?;
    let pairing_store = Arc::new(PairingStore::new()?);

    // Retain a reference for pairing code generation
    gateway.set_pairing_store(pairing_store.clone()).await;

    let config = GatewayConfig {
        gateway_type: gateway.gateway_type().to_string(),
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
