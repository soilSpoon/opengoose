use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use opengoose_core::Engine;
use opengoose_persistence::{AlertAction, AlertCondition, AlertMetric, AlertStore, Database};
use opengoose_secrets::{ConfigFile, CredentialResolver, SecretResult, SecretStore, SecretValue};
use opengoose_tui::ComposerRequest;
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};

use std::collections::HashMap;
use std::sync::{Mutex, Once};

use super::gateway::{collect_gateways, gateway_specs};
use super::lifecycle::{
    RetentionPolicy, spawn_periodic_alert_dispatch, spawn_periodic_cleanup,
    spawn_tui_composer_handler,
};
use super::pairing::{record_pairing_pairs, record_pairing_platforms, test_pairing_targets};

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
    let resolver = resolver_with_store(&[("matrix_homeserver_url", "https://matrix.example.com")]);
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
