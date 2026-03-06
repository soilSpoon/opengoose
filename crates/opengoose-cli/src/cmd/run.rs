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

/// Helper to push a gateway + bridge pair.
fn register(
    gateways: &mut Vec<Arc<dyn Gateway>>,
    bridges: &mut Vec<Arc<GatewayBridge>>,
    gw: Arc<dyn Gateway>,
    bridge: Arc<GatewayBridge>,
) {
    gateways.push(gw);
    bridges.push(bridge);
}

/// Attempt to create a Discord gateway from available credentials.
async fn try_discord(
    resolver: &CredentialResolver,
    engine: &Arc<Engine>,
    event_bus: &EventBus,
) -> Option<(Arc<dyn Gateway>, Arc<GatewayBridge>)> {
    let cred = resolver
        .resolve_async(&SecretKey::DiscordBotToken)
        .await
        .ok()?;
    let bridge = Arc::new(GatewayBridge::new(engine.clone()));
    let gw: Arc<dyn Gateway> = Arc::new(DiscordGateway::new(
        cred.value.as_str(),
        bridge.clone(),
        event_bus.clone(),
    ));
    Some((gw, bridge))
}

/// Attempt to create a Telegram gateway from available credentials.
async fn try_telegram(
    resolver: &CredentialResolver,
    engine: &Arc<Engine>,
    event_bus: &EventBus,
) -> Option<(Arc<dyn Gateway>, Arc<GatewayBridge>)> {
    let cred = resolver
        .resolve_async(&SecretKey::TelegramBotToken)
        .await
        .ok()?;
    let bridge = Arc::new(GatewayBridge::new(engine.clone()));
    let gw: Arc<dyn Gateway> = Arc::new(TelegramGateway::new(
        cred.value.as_str(),
        bridge.clone(),
        event_bus.clone(),
    ));
    Some((gw, bridge))
}

/// Attempt to create a Slack gateway from available credentials (requires both tokens).
async fn try_slack(
    resolver: &CredentialResolver,
    engine: &Arc<Engine>,
    event_bus: &EventBus,
) -> Option<(Arc<dyn Gateway>, Arc<GatewayBridge>)> {
    let bot_cred = resolver
        .resolve_async(&SecretKey::SlackBotToken)
        .await
        .ok()?;
    let app_cred = resolver
        .resolve_async(&SecretKey::SlackAppToken)
        .await
        .ok()?;
    let bridge = Arc::new(GatewayBridge::new(engine.clone()));
    let gw: Arc<dyn Gateway> = Arc::new(SlackGateway::new(
        app_cred.value.as_str(),
        bot_cred.value.as_str(),
        bridge.clone(),
        event_bus.clone(),
    ));
    Some((gw, bridge))
}

/// A gateway-specific result: the created gateway + its bridge, or `None` if credentials are missing.
type GatewayResult<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Option<(Arc<dyn Gateway>, Arc<GatewayBridge>)>> + 'a>,
>;

/// Async factory function that attempts to create a gateway from credentials.
type GatewayFactoryFn =
    for<'a> fn(&'a CredentialResolver, &'a Arc<Engine>, &'a EventBus) -> GatewayResult<'a>;

/// Registry of gateway factory functions.
///
/// To add a new channel, add a single entry here — no other changes needed
/// in this file.
const GATEWAY_FACTORIES: &[GatewayFactoryFn] = &[
    |r, e, b| Box::pin(try_discord(r, e, b)),
    |r, e, b| Box::pin(try_telegram(r, e, b)),
    |r, e, b| Box::pin(try_slack(r, e, b)),
];

/// Collect all gateways for which credentials are available.
async fn collect_gateways(
    resolver: &CredentialResolver,
    engine: Arc<Engine>,
    event_bus: &EventBus,
) -> (Vec<Arc<dyn Gateway>>, Vec<Arc<GatewayBridge>>) {
    let mut gateways: Vec<Arc<dyn Gateway>> = vec![];
    let mut bridges: Vec<Arc<GatewayBridge>> = vec![];

    for factory in GATEWAY_FACTORIES {
        if let Some((gw, bridge)) = factory(resolver, &engine, event_bus).await {
            register(&mut gateways, &mut bridges, gw, bridge);
        }
    }

    (gateways, bridges)
}

/// Spawn a task that listens for pairing code generation requests.
///
/// Generates a pairing code on ALL bridges so that any connected channel
/// can serve the pairing flow — not just the first gateway.
fn spawn_pairing_handler(
    bridges: Vec<Arc<GatewayBridge>>,
    platforms: Vec<String>,
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
                            for (bridge, platform) in bridges.iter().zip(platforms.iter()) {
                                if let Err(e) = bridge.generate_pairing_code(platform).await {
                                    tracing::error!(%e, %platform, "failed to generate pairing code");
                                }
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
                        let platforms: Vec<String> = gateways.iter().map(|g| g.gateway_type().to_string()).collect();
                        spawn_pairing_handler(bridges.to_vec(), platforms, pairing_rx, cancel.clone());
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
        let platforms: Vec<String> = gateways
            .iter()
            .map(|g| g.gateway_type().to_string())
            .collect();
        spawn_pairing_handler(bridges.to_vec(), platforms, pairing_rx, cancel.clone());
        start_gateways(gateways, bridges, cancel.clone()).await?;
        spawn_periodic_cleanup(engine.clone(), cancel.clone());

        opengoose_tui::run_tui(event_bus, cancel, AppMode::Normal, None, Some(pairing_tx)).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::sync::{Mutex, Once, OnceLock};

    use goose::config::Config;
    use goose::gateway::pairing::PairingStore;
    use opengoose_secrets::{
        ConfigFile, CredentialResolver, SecretResult, SecretStore, SecretValue,
    };
    use opengoose_types::AppEventKind;

    static RUSTLS_INIT: Once = Once::new();
    static GOOSE_PATH_ROOT: OnceLock<std::path::PathBuf> = OnceLock::new();

    struct MockStore {
        secrets: Mutex<HashMap<String, String>>,
    }

    impl MockStore {
        fn new(entries: &[(&str, &str)]) -> Self {
            let secrets = entries
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect();
            Self {
                secrets: Mutex::new(secrets),
            }
        }
    }

    impl SecretStore for MockStore {
        fn get(&self, key: &str) -> SecretResult<Option<SecretValue>> {
            Ok(self
                .secrets
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .map(SecretValue::new))
        }

        fn set(&self, key: &str, value: &str) -> SecretResult<()> {
            self.secrets
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, key: &str) -> SecretResult<bool> {
            Ok(self.secrets.lock().unwrap().remove(key).is_some())
        }
    }

    fn test_engine(event_bus: EventBus) -> Arc<Engine> {
        RUSTLS_INIT.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });

        Arc::new(Engine::new(event_bus, Database::open_in_memory().unwrap()))
    }

    fn ensure_goose_test_root() {
        let root = GOOSE_PATH_ROOT.get_or_init(|| {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("opengoose-cli-goose-{unique}"));
            std::fs::create_dir_all(&root).unwrap();
            // Safety: set once during test initialization, before Goose config is used.
            unsafe {
                std::env::set_var("GOOSE_PATH_ROOT", &root);
                std::env::set_var("GOOSE_DISABLE_KEYRING", "1");
            }
            root
        });

        std::fs::create_dir_all(root).unwrap();
        let config = Config::global();
        if config.exists() {
            let _ = config.clear();
        }
    }

    fn resolver_with_store(entries: &[(&str, &str)]) -> CredentialResolver {
        CredentialResolver::with_config_and_store(
            ConfigFile::default(),
            Arc::new(MockStore::new(entries)),
        )
    }

    #[tokio::test]
    async fn collect_gateways_returns_empty_when_no_credentials_are_available() {
        let resolver = resolver_with_store(&[]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        assert!(gateways.is_empty());
        assert!(bridges.is_empty());
    }

    #[tokio::test]
    async fn collect_gateways_builds_all_supported_gateways_from_credentials() {
        let resolver = resolver_with_store(&[
            ("discord_bot_token", "discord-token"),
            ("telegram_bot_token", "telegram-token"),
            ("slack_bot_token", "slack-bot-token"),
            ("slack_app_token", "slack-app-token"),
        ]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        let gateway_types: Vec<_> = gateways
            .iter()
            .map(|gateway| gateway.gateway_type())
            .collect();

        assert_eq!(gateway_types, vec!["discord", "telegram", "slack"]);
        assert_eq!(bridges.len(), 3);
    }

    #[tokio::test]
    async fn collect_gateways_skips_slack_without_both_required_tokens() {
        let resolver = resolver_with_store(&[("slack_bot_token", "slack-bot-token")]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        assert!(gateways.is_empty());
        assert!(bridges.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spawn_pairing_handler_generates_codes_on_request() {
        ensure_goose_test_root();

        let event_bus = EventBus::new(16);
        let mut events = event_bus.subscribe();
        let bridge = Arc::new(GatewayBridge::new(test_engine(event_bus.clone())));
        let store = Arc::new(PairingStore::new().unwrap());
        bridge.set_pairing_store(store.clone()).await;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        spawn_pairing_handler(
            vec![bridge],
            vec!["discord".to_string()],
            rx,
            cancel.clone(),
        );

        tx.send(()).unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
            .await
            .unwrap()
            .unwrap();
        let code = match event.kind {
            AppEventKind::PairingCodeGenerated { code } => code,
            other => panic!("expected pairing code event, got {}", other),
        };

        assert_eq!(
            store.consume_pending_code(&code).await.unwrap(),
            Some("discord".into())
        );

        cancel.cancel();
    }
}
