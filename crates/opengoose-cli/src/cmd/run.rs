use std::sync::Arc;

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use opengoose_core::{Engine, OpenGooseGateway, start_gateway};
use opengoose_discord::DiscordAdapter;
use opengoose_persistence::Database;
use opengoose_secrets::{CredentialResolver, SecretKey};
use opengoose_tui::{AppMode, TuiTracingLayer};
use opengoose_types::{EventBus, SessionKey};

async fn launch_discord(
    token: String,
    engine: Arc<Engine>,
    cancel: CancellationToken,
) -> Result<Arc<OpenGooseGateway>> {
    let (response_tx, response_rx) = tokio::sync::mpsc::channel::<(SessionKey, String)>(256);

    let gateway = Arc::new(OpenGooseGateway::new(
        response_tx,
        engine.clone(),
        "discord",
    ));

    // Initialize Goose agent system and register our gateway
    start_gateway(gateway.clone(), cancel.clone()).await?;

    let adapter = DiscordAdapter::new(
        token,
        gateway.clone(),
        response_rx,
        engine.event_bus().clone(),
    );

    // Run Discord adapter in background
    let cancel_discord = cancel.clone();
    tokio::spawn(async move {
        if let Err(e) = adapter.run(cancel_discord).await {
            tracing::error!(%e, "discord adapter error");
        }
    });

    Ok(gateway)
}

/// Spawn a task that listens for pairing code generation requests
/// and calls the gateway to generate them.
fn spawn_pairing_handler(
    gateway: Arc<OpenGooseGateway>,
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
                            if let Err(e) = gateway.generate_pairing_code().await {
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
    let mut pairing_rx = Some(pairing_rx);

    let resolver = CredentialResolver::new()?;
    match resolver.resolve_async(&SecretKey::DiscordBotToken).await {
        Ok(cred) => {
            // Token found — launch Discord immediately, then run TUI in Normal mode
            let token = cred.value.as_str().to_string();
            let gateway = launch_discord(token, engine.clone(), cancel.clone()).await?;

            if let Some(rx) = pairing_rx.take() {
                spawn_pairing_handler(gateway, rx, cancel.clone());
            }
            spawn_periodic_cleanup(engine.clone(), cancel.clone());

            opengoose_tui::run_tui(event_bus, cancel, AppMode::Normal, None, Some(pairing_tx))
                .await?;
        }
        Err(_) => {
            // No token — run TUI in Setup mode with oneshot channel
            let (tx, mut rx) = tokio::sync::oneshot::channel::<String>();

            // Create pairing channel upfront so TUI can request codes after setup completes
            let (pairing_tx, pairing_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

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
                    if let Ok(token) = token_result {
                        let gateway =
                            launch_discord(token, engine.clone(), cancel.clone()).await?;

                        spawn_pairing_handler(gateway, pairing_rx, cancel.clone());
                        spawn_periodic_cleanup(engine, cancel.clone());
                    }
                    // In both Ok and Err cases, wait for TUI to finish
                    tui_handle.await??;
                }
                tui_result = &mut tui_handle => {
                    // TUI exited before sending a token (user pressed q)
                    tui_result??;
                }
            }
        }
    }

    Ok(())
}
