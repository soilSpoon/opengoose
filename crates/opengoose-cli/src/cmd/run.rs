use std::sync::Arc;

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use opengoose_core::{start_gateway, OpenGooseGateway};
use opengoose_discord::DiscordAdapter;
use opengoose_secrets::{CredentialResolver, SecretKey};
use opengoose_tui::{AppMode, TuiTracingLayer};
use opengoose_types::{EventBus, SessionKey};

async fn launch_discord(
    token: String,
    event_bus: EventBus,
    cancel: CancellationToken,
) -> Result<Arc<OpenGooseGateway>> {
    let (response_tx, response_rx) =
        tokio::sync::mpsc::unbounded_channel::<(SessionKey, String)>();

    let gateway = Arc::new(OpenGooseGateway::new(response_tx, event_bus.clone()));

    // Initialize Goose agent system and register our gateway
    start_gateway(gateway.clone(), cancel.clone()).await?;

    let adapter = DiscordAdapter::new(token, gateway.clone(), response_rx, event_bus.clone());

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

    let resolver = CredentialResolver::new()?;
    match resolver.resolve_async(&SecretKey::DiscordBotToken).await {
        Ok(cred) => {
            // Token found — launch Discord immediately, then run TUI in Normal mode
            let token = cred.value.as_str().to_string();
            let gateway = launch_discord(token, event_bus.clone(), cancel.clone()).await?;

            let (pairing_tx, pairing_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
            spawn_pairing_handler(gateway, pairing_rx, cancel.clone());

            opengoose_tui::run_tui(
                event_bus,
                cancel,
                AppMode::Normal,
                None,
                Some(pairing_tx),
            )
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
                opengoose_tui::run_tui(tui_bus, tui_cancel, AppMode::Setup, Some(tx), Some(pairing_tx)).await
            });

            // Wait for either the token or TUI exit
            tokio::select! {
                token_result = &mut rx => {
                    if let Ok(token) = token_result {
                        let gateway =
                            launch_discord(token, event_bus, cancel.clone()).await?;

                        spawn_pairing_handler(gateway, pairing_rx, cancel.clone());
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
