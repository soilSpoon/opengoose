use super::GatewayBridge;
use super::pairing::PAIRING_PROMPT;

use std::sync::Arc;
use std::sync::OnceLock;

use goose::config::Config;
use goose::gateway::pairing::PairingStore;
use opengoose_persistence::Database;
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};
use tokio::sync::broadcast::error::TryRecvError;
use uuid::Uuid;

use crate::engine::Engine;

static GOOSE_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static GOOSE_PATH_ROOT: OnceLock<std::path::PathBuf> = OnceLock::new();

fn ensure_goose_test_root() {
    let root = GOOSE_PATH_ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("opengoose-goose-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        // Safety: tests serialize access with GOOSE_ENV_LOCK before any Goose config is initialized.
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

fn test_key() -> SessionKey {
    SessionKey::direct(Platform::Discord, "user-1")
}

fn test_engine(event_bus: EventBus) -> Arc<Engine> {
    Arc::new(Engine::new_with_team_store(
        event_bus,
        Database::open_in_memory().unwrap(),
        None,
    ))
}

#[tokio::test]
async fn generate_pairing_code_requires_store() {
    let event_bus = EventBus::new(16);
    let bridge = GatewayBridge::new(test_engine(event_bus));

    let err = bridge.generate_pairing_code("discord").await.unwrap_err();

    assert!(matches!(
        err,
        crate::error::GatewayError::PairingStoreNotReady
    ));
}

#[tokio::test]
async fn generate_pairing_code_persists_and_emits_event() {
    let _guard = GOOSE_ENV_LOCK.lock().await;
    ensure_goose_test_root();

    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let bridge = GatewayBridge::new(test_engine(event_bus));
    let store = Arc::new(PairingStore::new().unwrap());
    bridge.set_pairing_store(store.clone()).await;

    let code = bridge.generate_pairing_code("discord").await.unwrap();

    assert_eq!(code.len(), 6);
    assert_eq!(
        store.consume_pending_code(&code).await.unwrap(),
        Some("discord".into())
    );
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::PairingCodeGenerated { code: emitted } if emitted == code
    ));
}

#[tokio::test]
async fn outgoing_message_persists_history_and_pairing_events() {
    let _guard = GOOSE_ENV_LOCK.lock().await;
    ensure_goose_test_root();

    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let bridge = GatewayBridge::new(test_engine(event_bus));
    let key = test_key();
    let body = "Paired! You can now chat with goose.";

    bridge
        .on_outgoing_message(&key.to_stable_id(), body, "discord")
        .await;

    let history = bridge.sessions().load_history(&key, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].role, "assistant");
    assert_eq!(history[0].content, body);
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::PairingCompleted { session_key } if session_key == key
    ));
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::ResponseSent { session_key, content } if session_key == key && content == body
    ));
}

#[tokio::test]
async fn outgoing_pairing_prompt_auto_generates_code() {
    let _guard = GOOSE_ENV_LOCK.lock().await;
    ensure_goose_test_root();

    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let bridge = GatewayBridge::new(test_engine(event_bus));
    let store = Arc::new(PairingStore::new().unwrap());
    bridge.set_pairing_store(store.clone()).await;
    let key = test_key();

    bridge
        .on_outgoing_message(&key.to_stable_id(), PAIRING_PROMPT, "discord")
        .await;

    let pairing = rx.try_recv().unwrap();
    let code = match pairing.kind {
        AppEventKind::PairingCodeGenerated { code } => code,
        other => unreachable!("expected pairing code event, got {}", other),
    };
    assert_eq!(
        store.consume_pending_code(&code).await.unwrap(),
        Some("discord".into())
    );
    let response = rx.try_recv().unwrap();
    assert!(matches!(
        response.kind,
        AppEventKind::ResponseSent { session_key, content }
        if session_key == key && content == PAIRING_PROMPT
    ));
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn route_outgoing_text_returns_channel_id_and_emits_pairing_events() {
    let _guard = GOOSE_ENV_LOCK.lock().await;
    ensure_goose_test_root();

    let event_bus = EventBus::new(16);
    let mut rx = event_bus.subscribe();
    let bridge = GatewayBridge::new(test_engine(event_bus));
    let store = Arc::new(PairingStore::new().unwrap());
    bridge.set_pairing_store(store.clone()).await;
    let key = test_key();

    let channel_id = bridge
        .route_outgoing_text(&key.to_stable_id(), PAIRING_PROMPT, "discord")
        .await;

    assert_eq!(channel_id, key.channel_id);
    let pairing = rx.try_recv().unwrap();
    let code = match pairing.kind {
        AppEventKind::PairingCodeGenerated { code } => code,
        other => unreachable!("expected pairing code event, got {}", other),
    };
    assert_eq!(
        store.consume_pending_code(&code).await.unwrap(),
        Some("discord".into())
    );
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::ResponseSent { session_key, content }
        if session_key == key && content == PAIRING_PROMPT
    ));
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    let channel_id = bridge
        .route_outgoing_text(
            &key.to_stable_id(),
            "Paired! You can now chat with goose.",
            "discord",
        )
        .await;

    assert_eq!(channel_id, key.channel_id);
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::PairingCompleted { session_key } if session_key == key
    ));
    assert!(matches!(
        rx.try_recv().unwrap().kind,
        AppEventKind::ResponseSent { session_key, content }
        if session_key == key && content == "Paired! You can now chat with goose."
    ));
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

// ── handle_pairing (centralized team/profile routing) ────────────────────

fn test_engine_with_teams() -> Arc<Engine> {
    use opengoose_teams::TeamStore;
    use uuid::Uuid;
    let event_bus = EventBus::new(16);
    let dir = std::env::temp_dir().join(format!("opengoose-bridge-team-store-{}", Uuid::new_v4()));
    let store = TeamStore::with_dir(dir);
    store.install_defaults(false).unwrap();
    Arc::new(Engine::new_with_team_store(
        event_bus,
        Database::open_in_memory().unwrap(),
        Some(store),
    ))
}

#[test]
fn bridge_pairing_no_active_team() {
    let bridge = GatewayBridge::new(test_engine(EventBus::new(16)));
    let key = test_key();
    assert_eq!(
        bridge.handle_pairing(&key, ""),
        "No team active for this channel."
    );
}

#[test]
fn bridge_pairing_list_delegates_to_engine() {
    let bridge = GatewayBridge::new(test_engine_with_teams());
    let key = test_key();
    let response = bridge.handle_pairing(&key, "list");
    assert!(response.starts_with("Available teams:"), "{response}");
    assert!(response.contains("code-review"), "{response}");
}

#[test]
fn bridge_pairing_activate_and_deactivate() {
    let bridge = GatewayBridge::new(test_engine_with_teams());
    let key = test_key();

    let activate = bridge.handle_pairing(&key, "code-review");
    assert_eq!(activate, "Team code-review activated for this channel.");

    let status = bridge.handle_pairing(&key, "");
    assert_eq!(status, "Active team: code-review");

    let deactivate = bridge.handle_pairing(&key, "off");
    assert_eq!(
        deactivate,
        "Team deactivated. Reverting to single-agent mode."
    );

    let empty = bridge.handle_pairing(&key, "");
    assert_eq!(empty, "No team active for this channel.");
}

#[test]
fn bridge_pairing_unknown_team_reports_available() {
    let bridge = GatewayBridge::new(test_engine_with_teams());
    let key = test_key();
    let response = bridge.handle_pairing(&key, "nonexistent");
    assert!(
        response.contains("not found"),
        "expected 'not found' in: {response}"
    );
}
