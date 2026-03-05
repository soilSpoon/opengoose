use std::sync::Arc;

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use goose::gateway::Gateway;
use opengoose_core::{Engine, GatewayBridge, start_gateways};
use opengoose_discord::DiscordGateway;
use opengoose_persistence::Database;
use opengoose_secrets::{CredentialResolver, SecretKey};
use opengoose_slack::SlackGateway;
use opengoose_telegram::TelegramGateway;
use opengoose_tui::{AppMode, TuiTracingLayer};
use opengoose_types::EventBus;

/// Collect all gateways for which credentials are available.
async fn collect_gateways(
    resolver: &CredentialResolver,
    engine: Arc<Engine>,
    event_bus: &EventBus,
) -> (Vec<Arc<dyn Gateway>>, Vec<Arc<GatewayBridge>>) {
    let mut gateways: Vec<Arc<dyn Gateway>> = vec![];
    let mut bridges: Vec<Arc<GatewayBridge>> = vec![];

    // Discord
    if let Ok(cred) = resolver.resolve_async(&SecretKey::DiscordBotToken).await {
        let bridge = Arc::new(GatewayBridge::new(engine.clone()));
        let gw = Arc::new(DiscordGateway::new(
            cred.value.as_str(),
            bridge.clone(),
            event_bus.clone(),
        ));
        gateways.push(gw);
        bridges.push(bridge);
    }

    // Telegram
    if let Ok(cred) = resolver.resolve_async(&SecretKey::TelegramBotToken).await {
        let bridge = Arc::new(GatewayBridge::new(engine.clone()));
        let gw = Arc::new(TelegramGateway::new(
            cred.value.as_str(),
            bridge.clone(),
            event_bus.clone(),
        ));
        gateways.push(gw);
        bridges.push(bridge);
    }

    // Slack (requires both app token and bot token)
    if let Ok(bot_cred) = resolver.resolve_async(&SecretKey::SlackBotToken).await {
        if let Ok(app_cred) = resolver.resolve_async(&SecretKey::SlackAppToken).await {
            let bridge = Arc::new(GatewayBridge::new(engine.clone()));
            let gw = Arc::new(SlackGateway::new(
                app_cred.value.as_str(),
                bot_cred.value.as_str(),
                bridge.clone(),
                event_bus.clone(),
            ));
            gateways.push(gw);
            bridges.push(bridge);
        }
    }

    (gateways, bridges)
}

/// Spawn a task that listens for pairing code generation requests.
fn spawn_pairing_handler(
    bridge: Arc<GatewayBridge>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                req = rx.recv() => {
                    match req {
                        Some(()) => {
                            if let Err(e) = bridge.generate_pairing_code("opengoose").await {
                                tracing::error!(%e, "failed to generate pairing code");
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });
}

/// Spawn a periodic cleanup task for old sessions (every hour, removes sessions older than 72h).
fn spawn_periodic_cleanup(engine: Arc<Engine>, cancel: CancellationToken) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    if let Err(e) = engine.sessions().cleanup(72) {
                        tracing::warn!(%e, "periodic session cleanup failed");
                    }
                }
            }
        }
    });
}

pub async fn execute() -> Result<()> {
    let event_bus = EventBus::new(256);

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
    let engine = Arc::new(Engine::new(event_bus.clone(), db));

    // Create the pairing channel upfront so the TUI can trigger pairing
    // code generation in both Normal and Setup→Normal flows.
    let (pairing_tx, pairing_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let resolver = CredentialResolver::new()?;
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
                        if let Some(bridge) = bridges.first() {
                            spawn_pairing_handler(bridge.clone(), pairing_rx, cancel.clone());
                        }
                        start_gateways(gateways, bridges, cancel.clone()).await?;
                        spawn_periodic_cleanup(engine, cancel.clone());
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
        if let Some(bridge) = bridges.first() {
            spawn_pairing_handler(bridge.clone(), pairing_rx, cancel.clone());
        }
        start_gateways(gateways, bridges, cancel.clone()).await?;
        spawn_periodic_cleanup(engine.clone(), cancel.clone());

        opengoose_tui::run_tui(event_bus, cancel, AppMode::Normal, None, Some(pairing_tx))
            .await?;
    }

    Ok(())
}
