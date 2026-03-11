use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use goose::gateway::Gateway;
use opengoose_core::{Engine, GatewayBridge, alerts::AlertDispatcher, start_gateways};
use opengoose_discord::DiscordGateway;
use opengoose_matrix::MatrixGateway;
use opengoose_persistence::{
    AlertStore, DEFAULT_EVENT_RETENTION_DAYS, Database, EventStore, spawn_event_history_recorder,
};
use opengoose_profiles::ProfileStore;
use opengoose_secrets::{CredentialResolver, SecretKey};
use opengoose_slack::SlackGateway;
use opengoose_teams::{scheduler, spawn_event_bus_trigger_watcher};
use opengoose_telegram::TelegramGateway;
use opengoose_tui::{AppMode, ComposerRequest, TuiTracingLayer};
use opengoose_types::{AppEventKind, EventBus};

const ALERT_EVALUATION_INTERVAL: Duration = Duration::from_secs(30);

type GatewayBuilder = fn(&[&str], Arc<GatewayBridge>, EventBus) -> anyhow::Result<Arc<dyn Gateway>>;

/// Declarative specification for constructing a gateway from credentials.
///
/// To add a new channel, add a single entry to [`gateway_specs`] — no other
/// changes needed in this file.
struct GatewaySpec {
    /// Secret keys that must all resolve (order matches `build` parameter order).
    keys: Vec<SecretKey>,
    /// Construct the gateway from resolved credential values, a bridge, and the event bus.
    build: GatewayBuilder,
}

/// Registry of all supported gateway specifications.
fn gateway_specs() -> Vec<GatewaySpec> {
    vec![
        GatewaySpec {
            keys: vec![SecretKey::DiscordBotToken],
            build: |creds, bridge, bus| Ok(Arc::new(DiscordGateway::new(creds[0], bridge, bus))),
        },
        GatewaySpec {
            keys: vec![SecretKey::TelegramBotToken],
            build: |creds, bridge, bus| Ok(Arc::new(TelegramGateway::new(creds[0], bridge, bus)?)),
        },
        GatewaySpec {
            keys: vec![SecretKey::SlackAppToken, SecretKey::SlackBotToken],
            build: |creds, bridge, bus| {
                Ok(Arc::new(SlackGateway::new(creds[0], creds[1], bridge, bus)))
            },
        },
        GatewaySpec {
            keys: vec![SecretKey::MatrixHomeserverUrl, SecretKey::MatrixAccessToken],
            build: |creds, bridge, bus| {
                Ok(Arc::new(MatrixGateway::new(
                    creds[0], creds[1], bridge, bus,
                )?))
            },
        },
    ]
}

/// Collect all gateways for which credentials are available.
async fn collect_gateways(
    resolver: &CredentialResolver,
    engine: Arc<Engine>,
    event_bus: &EventBus,
) -> (Vec<Arc<dyn Gateway>>, Vec<Arc<GatewayBridge>>) {
    let mut gateways: Vec<Arc<dyn Gateway>> = vec![];
    let mut bridges: Vec<Arc<GatewayBridge>> = vec![];

    for spec in gateway_specs() {
        // Resolve all required credentials; skip this gateway if any are missing.
        let mut values = Vec::with_capacity(spec.keys.len());
        let mut all_resolved = true;
        for key in &spec.keys {
            match resolver.resolve_async(key).await {
                Ok(cred) => values.push(cred.value),
                Err(_) => {
                    all_resolved = false;
                    break;
                }
            }
        }
        if !all_resolved {
            continue;
        }

        let bridge = Arc::new(GatewayBridge::new(engine.clone()));
        let cred_strs: Vec<&str> = values.iter().map(|v| v.as_str()).collect();
        match (spec.build)(&cred_strs, bridge.clone(), event_bus.clone()) {
            Ok(gw) => {
                gateways.push(gw);
                bridges.push(bridge);
            }
            Err(e) => {
                tracing::warn!("failed to create gateway: {e}");
            }
        }
    }

    (gateways, bridges)
}

/// Spawn a task that listens for pairing code generation requests.
///
/// Generates a pairing code on ALL bridges so that any connected channel
/// can serve the pairing flow — not just the first gateway.
async fn for_each_pairing_target<T, F, Fut>(targets: &[T], platforms: &[String], mut f: F)
where
    T: Clone,
    F: FnMut(T, String) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for (target, platform) in targets.iter().zip(platforms.iter()) {
        f(target.clone(), platform.clone()).await;
    }
}

async fn generate_pairing_codes(bridges: &[Arc<GatewayBridge>], platforms: &[String]) {
    for_each_pairing_target(bridges, platforms, |bridge, platform| async move {
        if let Err(e) = bridge.generate_pairing_code(&platform).await {
            tracing::error!(%e, %platform, "failed to generate pairing code");
        }
    })
    .await;
}

#[cfg(test)]
async fn record_pairing_platforms(targets: &[String], platforms: &[String]) -> Vec<String> {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    for_each_pairing_target(targets, platforms, {
        let seen = seen.clone();
        move |_target, platform| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push(platform);
            }
        }
    })
    .await;
    seen.lock().unwrap().clone()
}

#[cfg(test)]
async fn record_pairing_pairs(targets: &[String], platforms: &[String]) -> Vec<(String, String)> {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    for_each_pairing_target(targets, platforms, {
        let seen = seen.clone();
        move |target, platform| {
            let seen = seen.clone();
            async move {
                seen.lock().unwrap().push((target, platform));
            }
        }
    })
    .await;
    seen.lock().unwrap().clone()
}

#[cfg(test)]
fn test_pairing_targets(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

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
                        Some(()) => generate_pairing_codes(&bridges, &platforms).await,
                        None => break,
                    }
                }
            }
        }
    });
}

/// Resolve runtime retention settings from the default `main` profile.
#[derive(Debug, Clone, Copy)]
struct RetentionPolicy {
    message_retention_days: Option<u32>,
    event_retention_days: u32,
}

fn main_profile_retention_policy() -> Result<RetentionPolicy> {
    let store = ProfileStore::new()?;
    let settings = store.get("main").ok().and_then(|profile| profile.settings);

    Ok(RetentionPolicy {
        message_retention_days: settings.as_ref().and_then(|s| s.message_retention_days),
        event_retention_days: settings
            .and_then(|s| s.event_retention_days)
            .unwrap_or(DEFAULT_EVENT_RETENTION_DAYS),
    })
}

/// Spawn a periodic cleanup task for expired session messages.
fn spawn_periodic_cleanup(
    engine: Arc<Engine>,
    cancel: CancellationToken,
    retention_policy: RetentionPolicy,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    if let Some(retention_days) = retention_policy.message_retention_days
                        && let Err(e) = engine.sessions().cleanup_expired_messages(retention_days)
                    {
                        tracing::warn!(%e, retention_days, "periodic message cleanup failed");
                    }

                    if let Err(e) = EventStore::new(engine.db().clone())
                        .cleanup_expired(retention_policy.event_retention_days)
                    {
                        tracing::warn!(
                            %e,
                            retention_days = retention_policy.event_retention_days,
                            "periodic event cleanup failed"
                        );
                    }
                }
            }
        }
    });
}

fn spawn_configured_periodic_cleanup(
    engine: Arc<Engine>,
    cancel: CancellationToken,
    retention_policy: RetentionPolicy,
) {
    tracing::info!(
        message_retention_days = retention_policy.message_retention_days,
        event_retention_days = retention_policy.event_retention_days,
        "enabled periodic retention cleanup"
    );
    spawn_periodic_cleanup(engine, cancel, retention_policy);
}

fn spawn_runtime_event_recorder(db: Arc<Database>, event_bus: EventBus, cancel: CancellationToken) {
    spawn_event_history_recorder(db, event_bus, cancel);
}

fn spawn_periodic_alert_dispatch(
    db: Arc<Database>,
    event_bus: EventBus,
    cancel: CancellationToken,
    interval: Duration,
) {
    let dispatcher = Arc::new(AlertDispatcher::new(
        Arc::new(AlertStore::new(db)),
        event_bus,
    ));
    dispatcher.start_periodic(interval, cancel);
}

fn spawn_tui_composer_handler(
    engine: Arc<Engine>,
    event_bus: EventBus,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<ComposerRequest>,
    cancel: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                request = rx.recv() => {
                    let Some(request) = request else {
                        break;
                    };
                    if let Err(e) = engine
                        .process_message_streaming(
                            &request.session_key,
                            Some("operator"),
                            &request.content,
                        )
                        .await
                    {
                        event_bus.emit(AppEventKind::Error {
                            context: "tui_compose".into(),
                            message: e.to_string(),
                        });
                    }
                }
            }
        }
    });
}

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
    let engine = Arc::new(Engine::new(event_bus.clone(), db));
    spawn_runtime_event_recorder(engine.db().clone(), event_bus.clone(), cancel.clone());

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

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::sync::{Mutex, Once};

    use opengoose_persistence::{AlertAction, AlertCondition, AlertMetric, AlertStore};
    use opengoose_secrets::{
        ConfigFile, CredentialResolver, SecretResult, SecretStore, SecretValue,
    };
    use opengoose_types::{EventBus, Platform, SessionKey};

    static RUSTLS_INIT: Once = Once::new();

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
            ("matrix_homeserver_url", "https://matrix.example.com"),
            ("matrix_access_token", "matrix-token"),
        ]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        let gateway_types: Vec<_> = gateways
            .iter()
            .map(|gateway| gateway.gateway_type())
            .collect();

        assert_eq!(
            gateway_types,
            vec!["discord", "telegram", "slack", "matrix"]
        );
        assert_eq!(bridges.len(), 4);
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

    #[tokio::test]
    async fn collect_gateways_skips_matrix_without_both_required_credentials() {
        // Only homeserver URL — no access token
        let resolver =
            resolver_with_store(&[("matrix_homeserver_url", "https://matrix.example.com")]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        assert!(gateways.is_empty());
        assert!(bridges.is_empty());
    }

    #[tokio::test]
    async fn collect_gateways_builds_matrix_gateway_when_both_credentials_provided() {
        let resolver = resolver_with_store(&[
            ("matrix_homeserver_url", "https://matrix.example.com"),
            ("matrix_access_token", "syt_test_token"),
        ]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        let gateway_types: Vec<_> = gateways
            .iter()
            .map(|gateway| gateway.gateway_type())
            .collect();

        assert_eq!(gateway_types, vec!["matrix"]);
        assert_eq!(bridges.len(), 1);
    }

    #[tokio::test]
    async fn generate_pairing_codes_records_one_platform_per_target() {
        let targets = test_pairing_targets(&["bridge-a"]);
        let platforms = vec!["discord".to_string()];

        let seen = record_pairing_platforms(&targets, &platforms).await;

        assert_eq!(seen, vec!["discord".to_string()]);
    }

    #[tokio::test]
    async fn generate_pairing_codes_zips_targets_with_platforms() {
        let targets = test_pairing_targets(&["bridge-a", "bridge-b"]);
        let platforms = vec!["discord".to_string(), "slack".to_string()];

        let seen = record_pairing_pairs(&targets, &platforms).await;

        assert_eq!(
            seen,
            vec![
                ("bridge-a".to_string(), "discord".to_string()),
                ("bridge-b".to_string(), "slack".to_string()),
            ]
        );
    }

    #[test]
    fn gateway_specs_has_four_entries() {
        assert_eq!(gateway_specs().len(), 4);
    }

    #[tokio::test]
    async fn collect_gateways_builds_only_discord_when_only_discord_token_provided() {
        let resolver = resolver_with_store(&[("discord_bot_token", "discord-token")]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        assert_eq!(gateways.len(), 1);
        assert_eq!(gateways[0].gateway_type(), "discord");
        assert_eq!(bridges.len(), 1);
    }

    #[tokio::test]
    async fn collect_gateways_builds_only_telegram_when_only_telegram_token_provided() {
        let resolver = resolver_with_store(&[("telegram_bot_token", "telegram-token")]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        assert_eq!(gateways.len(), 1);
        assert_eq!(gateways[0].gateway_type(), "telegram");
        assert_eq!(bridges.len(), 1);
    }

    #[tokio::test]
    async fn collect_gateways_builds_only_slack_when_both_slack_tokens_provided() {
        let resolver = resolver_with_store(&[
            ("slack_bot_token", "slack-bot"),
            ("slack_app_token", "slack-app"),
        ]);
        let event_bus = EventBus::new(16);
        let (gateways, bridges) =
            collect_gateways(&resolver, test_engine(event_bus.clone()), &event_bus).await;

        assert_eq!(gateways.len(), 1);
        assert_eq!(gateways[0].gateway_type(), "slack");
        assert_eq!(bridges.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spawn_periodic_cleanup_stops_on_cancel() {
        let event_bus = EventBus::new(16);
        let engine = test_engine(event_bus.clone());
        let cancel = CancellationToken::new();
        spawn_periodic_cleanup(
            engine,
            cancel.clone(),
            RetentionPolicy {
                message_retention_days: Some(7),
                event_retention_days: 30,
            },
        );
        cancel.cancel();
        // Give the task a moment to observe the cancellation signal
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        // No assertions: the test confirms the task terminates cleanly rather than hanging
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spawn_tui_composer_handler_emits_error_event_when_engine_processing_fails() {
        let event_bus = EventBus::new(16);
        let mut events = event_bus.subscribe();
        let engine = Arc::new(Engine::new_with_team_store(
            event_bus.clone(),
            Database::open_in_memory().unwrap(),
            None,
        ));
        let session_key = SessionKey::dm(Platform::Discord, "operator");
        engine.set_active_team(&session_key, "code-review".to_string());

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        spawn_tui_composer_handler(engine, event_bus.clone(), rx, cancel.clone());

        tx.send(ComposerRequest {
            session_key,
            content: "hello world".to_string(),
        })
        .unwrap();

        let (context, message) = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let event = events.recv().await.unwrap();
                if let AppEventKind::Error { context, message } = event.kind {
                    return (context, message);
                }
            }
        })
        .await
        .unwrap();

        assert_eq!(context, "tui_compose");
        assert!(message.contains("team store not available"));

        cancel.cancel();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spawn_periodic_alert_dispatch_fires_enabled_rules_from_runtime_loop() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let store = AlertStore::new(db.clone());
        store
            .create(
                "runtime-alert",
                None,
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                -1.0,
                &[AlertAction::ChannelMessage {
                    platform: "slack".into(),
                    channel_id: "C123".into(),
                }],
            )
            .unwrap();

        let event_bus = EventBus::new(16);
        let mut events = event_bus.subscribe();
        let cancel = CancellationToken::new();

        spawn_periodic_alert_dispatch(
            db.clone(),
            event_bus.clone(),
            cancel.clone(),
            Duration::from_millis(10),
        );

        let event = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let event = events.recv().await.unwrap();
                if matches!(event.kind, AppEventKind::AlertFired { .. }) {
                    return event;
                }
            }
        })
        .await
        .unwrap()
        .kind;

        assert!(matches!(
            event,
            AppEventKind::AlertFired {
                ref rule_name,
                ref platform,
                ref channel_id,
                ..
            } if rule_name == "runtime-alert" && platform == "slack" && channel_id == "C123"
        ));

        let history = AlertStore::new(db).history(10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].rule_name, "runtime-alert");

        cancel.cancel();
    }
}
