//! Platform-agnostic core for OpenGoose: session management and AI streaming.
//!
//! The central types are:
//! - [`Engine`] — creates and drives AI sessions, routing messages between
//!   channel gateways and the Goose provider backend.
//! - [`GatewayBridge`] — wraps a [`Gateway`] to adapt it for `Engine` use.
//! - [`ThrottlePolicy`] — per-session request rate control.
//! - [`StreamResponder`] / [`DraftHandle`] — incremental response delivery.
//! - [`SessionManager`] — lifecycle tracking for active sessions.
//!
//! Channel adapters (Slack, Discord, Telegram, Matrix) consume this crate's
//! public API to integrate platform-specific transports with the AI engine.

pub mod alerts;
mod bridge;
mod engine;
mod error;
pub mod message_utils;
mod session_manager;
pub mod stream_orchestrator;
pub mod stream_responder;
pub mod throttle;

pub use bridge::GatewayBridge;
pub use engine::Engine;
pub use error::GatewayError;
pub use message_utils::{split_message, truncate_for_display};
pub use session_manager::SessionManager;
pub use stream_responder::{DraftHandle, StreamResponder};
pub use throttle::ThrottlePolicy;

use std::sync::Arc;

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
    cancel: tokio_util::sync::CancellationToken,
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home(test: impl FnOnce(&std::path::Path)) {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_home =
            std::env::temp_dir().join(format!("opengoose-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_home).unwrap();

        let saved_home = std::env::var("HOME").ok();
        let saved_recipe_path = std::env::var("GOOSE_RECIPE_PATH").ok();

        // Safety: serialized by ENV_LOCK and only used in single-threaded test setup.
        unsafe {
            std::env::set_var("HOME", &temp_home);
            std::env::remove_var("GOOSE_RECIPE_PATH");
        }

        test(&temp_home);

        unsafe {
            match saved_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match saved_recipe_path {
                Some(value) => std::env::set_var("GOOSE_RECIPE_PATH", value),
                None => std::env::remove_var("GOOSE_RECIPE_PATH"),
            }
        }

        let _ = std::fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn setup_profiles_and_teams_installs_defaults_and_registers_recipe_path() {
        with_temp_home(|home| {
            setup_profiles_and_teams().unwrap();

            let profile_store = opengoose_profiles::ProfileStore::new().unwrap();
            let team_store = opengoose_teams::TeamStore::new().unwrap();

            assert_eq!(profile_store.list().unwrap().len(), 9);
            assert_eq!(team_store.list().unwrap().len(), 7);
            assert_eq!(
                std::env::var("GOOSE_RECIPE_PATH").unwrap(),
                home.join(".opengoose")
                    .join("profiles")
                    .display()
                    .to_string()
            );
        });
    }
}
