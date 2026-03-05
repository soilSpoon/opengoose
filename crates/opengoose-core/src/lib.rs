mod bridge;
mod engine;
mod error;
mod gateway;
mod session_manager;

pub use bridge::GatewayBridge;
pub use engine::Engine;
pub use error::GatewayError;
pub use gateway::OpenGooseGateway;
pub use session_manager::SessionManager;

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
        info!(
            count = teams_installed,
            "installed default team definitions"
        );
    }

    Ok(())
}

/// Initialize Goose agent system and wire up a single gateway.
/// Uses Goose's default config paths (~/.config/goose/).
///
/// Assumes `setup_profiles_and_teams()` has already been called.
///
/// **Legacy API** — prefer [`start_gateways`] for multi-channel support.
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

/// Initialize the Goose agent system and start multiple channel gateways.
///
/// Each gateway gets its own `GatewayHandler` but they all share the same
/// `AgentManager` and `PairingStore`, ensuring a unified agent system across
/// all channels.
///
/// Each gateway is spawned as an independent tokio task so that one channel
/// failing does not bring down the others.
pub async fn start_gateways(
    gateways: Vec<Arc<dyn Gateway>>,
    bridges: Vec<Arc<GatewayBridge>>,
    cancel: CancellationToken,
) -> Result<(), GatewayError> {
    let agent_manager = AgentManager::instance().await?;
    let pairing_store = Arc::new(PairingStore::new()?);

    // Give each bridge access to the shared pairing store
    for bridge in &bridges {
        bridge.set_pairing_store(pairing_store.clone()).await;
    }

    info!(
        count = gateways.len(),
        "starting goose agent system with multiple gateways"
    );

    for gateway in gateways {
        let config = GatewayConfig {
            gateway_type: gateway.gateway_type().to_string(),
            platform_config: serde_json::json!({}),
            max_sessions: 100,
        };

        let handler = GatewayHandler::new(
            agent_manager.clone(),
            pairing_store.clone(),
            gateway.clone(),
            config,
        );

        let cancel = cancel.clone();
        let gw_type = gateway.gateway_type().to_string();
        tokio::spawn(async move {
            info!(gateway = %gw_type, "starting gateway");
            if let Err(e) = gateway.start(handler, cancel).await {
                tracing::error!(gateway = %gw_type, %e, "gateway error");
            }
        });
    }

    Ok(())
}
