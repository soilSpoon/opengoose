mod gateway;
mod lifecycle;
mod pairing;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use opengoose_core::start_gateways;
use opengoose_persistence::Database;
use opengoose_teams::{scheduler, spawn_event_bus_trigger_watcher};
use opengoose_tui::{AppMode, ComposerRequest, TuiTracingLayer};
use opengoose_types::EventBus;

use gateway::collect_gateways;
use lifecycle::{
    main_profile_retention_policy, spawn_configured_periodic_cleanup,
    spawn_periodic_alert_dispatch, spawn_runtime_event_recorder, spawn_tui_composer_handler,
};
use pairing::spawn_pairing_handler;

const ALERT_EVALUATION_INTERVAL: Duration = Duration::from_secs(30);

/// Start channel gateways (Discord, Telegram, Slack, Matrix) and the TUI.
///
/// Enters Setup mode when no credentials are found, switching to Normal mode
/// after the user completes first-time configuration.
pub async fn execute() -> Result<()> {
    let event_bus = EventBus::new(256);
    let retention_policy = main_profile_retention_policy()?;

    // Use TUI tracing layer instead of fmt — logs go to the events panel
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,opengoose=debug".parse().unwrap()),
        )
        .with(TuiTracingLayer::new(event_bus.clone()))
        .init();

    let cancel = CancellationToken::new();

    // Spawn signal handler for graceful shutdown
    let cancel_for_signal = cancel.clone();
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            tracing::info!("received Ctrl+C, shutting down...");
            cancel_for_signal.cancel();
        }
    });

    // Initialize shared database
    let db = Database::open()?;

    // Create the platform-agnostic engine (runs initial cleanup + suspends incomplete runs)
    let engine = Arc::new(opengoose_core::Engine::new(event_bus.clone(), db));
    let _recorder = spawn_runtime_event_recorder(engine.db().clone(), event_bus.clone());

    // Create the pairing channel upfront so the TUI can trigger pairing
    // code generation in both Normal and Setup→Normal flows.
    let (pairing_tx, pairing_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let (composer_tx, composer_rx) = tokio::sync::mpsc::unbounded_channel::<ComposerRequest>();
    spawn_tui_composer_handler(
        engine.clone(),
        event_bus.clone(),
        composer_rx,
        cancel.clone(),
    );

    let resolver = opengoose_secrets::CredentialResolver::new()?;
    let (gateways, bridges) = collect_gateways(&resolver, engine.clone(), &event_bus).await;

    if gateways.is_empty() {
        // No credentials found — run TUI in Setup mode
        let (tx, mut rx) = tokio::sync::oneshot::channel::<String>();

        let tui_bus = event_bus.clone();
        let tui_cancel = cancel.clone();
        let mut tui_handle = tokio::spawn(async move {
            opengoose_tui::run_tui(
                tui_bus,
                tui_cancel,
                AppMode::Setup,
                Some(tx),
                Some(pairing_tx),
                Some(composer_tx.clone()),
            )
            .await
        });

        // Wait for either the token or TUI exit
        tokio::select! {
            token_result = &mut rx => {
                if let Ok(_token) = token_result {
                    // Re-collect gateways after setup provides credentials
                    let (gateways, bridges) =
                        collect_gateways(&resolver, engine.clone(), &event_bus).await;

                    if !gateways.is_empty() {
                        let platforms: Vec<String> = gateways.iter().map(|g| g.gateway_type().to_string()).collect();
                        spawn_pairing_handler(bridges.to_vec(), platforms, pairing_rx, cancel.clone());
                        start_gateways(gateways, bridges, cancel.clone()).await?;
                        spawn_configured_periodic_cleanup(
                            engine.clone(),
                            cancel.clone(),
                            retention_policy,
                        );
                        scheduler::spawn_scheduler(engine.db().clone(), event_bus.clone(), cancel.clone());
                        spawn_event_bus_trigger_watcher(engine.db().clone(), event_bus.clone(), cancel.clone());
                        spawn_periodic_alert_dispatch(
                            engine.db().clone(),
                            event_bus.clone(),
                            cancel.clone(),
                            ALERT_EVALUATION_INTERVAL,
                        );
                    }
                }
                tui_handle.await??;
            }
            tui_result = &mut tui_handle => {
                tui_result??;
            }
        }
    } else {
        // Credentials found — launch all gateways and run TUI in Normal mode
        let platforms: Vec<String> = gateways
            .iter()
            .map(|g| g.gateway_type().to_string())
            .collect();
        spawn_pairing_handler(bridges.to_vec(), platforms, pairing_rx, cancel.clone());
        start_gateways(gateways, bridges, cancel.clone()).await?;
        spawn_configured_periodic_cleanup(engine.clone(), cancel.clone(), retention_policy);
        scheduler::spawn_scheduler(engine.db().clone(), event_bus.clone(), cancel.clone());
        spawn_event_bus_trigger_watcher(engine.db().clone(), event_bus.clone(), cancel.clone());
        spawn_periodic_alert_dispatch(
            engine.db().clone(),
            event_bus.clone(),
            cancel.clone(),
            ALERT_EVALUATION_INTERVAL,
        );
        opengoose_tui::run_tui(
            event_bus,
            cancel,
            AppMode::Normal,
            None,
            Some(pairing_tx),
            Some(composer_tx),
        )
        .await?;
    }

    Ok(())
}
