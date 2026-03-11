use opengoose_secrets::{SecretResult, SecretStore, SecretValue};
use opengoose_types::{Platform, SessionKey};
use tokio::sync::mpsc;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::state::{App, AppMode};
use crate::ComposerRequest;

struct MockStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl MockStore {
    fn new() -> Self {
        Self {
            secrets: Mutex::new(HashMap::new()),
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
            .map(|v| SecretValue::new(v.clone())))
    }

    fn set(&self, key: &str, value: &str) -> SecretResult<()> {
        self.secrets
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, key: &str) -> SecretResult<bool> {
        Ok(self.secrets.lock().unwrap().remove(key).is_some())
    }
}

fn test_app() -> App {
    App::with_store(
        AppMode::Normal,
        None,
        None,
        Arc::new(MockStore::new()),
        None,
    )
}

#[test]
fn test_set_composer_tx_sets_channel() {
    let mut app = test_app();
    assert!(app.composer_tx.is_none());

    let (tx, _rx) = mpsc::unbounded_channel::<ComposerRequest>();
    app.set_composer_tx(tx);

    assert!(app.composer_tx.is_some());
}

#[test]
fn test_composer_session_key_returns_default_when_no_selection() {
    let app = test_app();
    assert!(app.selected_session.is_none());

    let key = app.composer_session_key();
    // Default key uses the TUI platform and "local" channel id
    assert_eq!(key.channel_id, "local");
}

#[test]
fn test_composer_session_key_returns_selected_session() {
    let mut app = test_app();
    let expected = SessionKey::new(Platform::Discord, "guild-1", "chan-1");
    app.selected_session = Some(expected.clone());

    let key = app.composer_session_key();
    assert_eq!(key, expected);
}

#[test]
fn test_submit_composer_empty_input_does_nothing() {
    let mut app = test_app();
    let (tx, mut rx) = mpsc::unbounded_channel::<ComposerRequest>();
    app.set_composer_tx(tx);
    app.composer.input = "".into();

    app.submit_composer();

    // No message sent
    assert!(rx.try_recv().is_err());
}

#[test]
fn test_submit_composer_whitespace_only_does_nothing() {
    let mut app = test_app();
    let (tx, mut rx) = mpsc::unbounded_channel::<ComposerRequest>();
    app.set_composer_tx(tx);
    app.composer.input = "   \t\n".into();

    app.submit_composer();

    assert!(rx.try_recv().is_err());
}

#[test]
fn test_submit_composer_no_tx_pushes_error_event() {
    let mut app = test_app();
    app.composer_tx = None;
    app.composer.input = "hello world".into();

    app.submit_composer();

    let last_event = app.events.back().unwrap();
    assert!(
        last_event.summary.contains("unavailable"),
        "expected unavailable message, got: {}",
        last_event.summary
    );
}

#[test]
fn test_submit_composer_sends_message_and_clears_input() {
    let mut app = test_app();
    let (tx, mut rx) = mpsc::unbounded_channel::<ComposerRequest>();
    app.set_composer_tx(tx);
    app.composer.input = "hello goose".into();

    app.submit_composer();

    let request = rx.try_recv().expect("message should have been sent");
    assert_eq!(request.content, "hello goose");
    assert!(app.composer.input.is_empty());
}

#[test]
fn test_submit_composer_adds_to_history() {
    let mut app = test_app();
    let (tx, _rx) = mpsc::unbounded_channel::<ComposerRequest>();
    app.set_composer_tx(tx);
    app.composer.input = "first message".into();

    app.submit_composer();

    // After submission, history should contain the sent message
    assert!(!app.composer.history.is_empty());
    assert_eq!(app.composer.history[0], "first message");
}

#[test]
fn test_submit_composer_uses_selected_session_key() {
    let mut app = test_app();
    let expected_key = SessionKey::new(Platform::Discord, "guild-1", "chan-1");
    app.selected_session = Some(expected_key.clone());

    let (tx, mut rx) = mpsc::unbounded_channel::<ComposerRequest>();
    app.set_composer_tx(tx);
    app.composer.input = "ping".into();

    app.submit_composer();

    let request = rx.try_recv().unwrap();
    assert_eq!(request.session_key, expected_key);
}

#[test]
fn test_submit_composer_default_session_key_uses_tui_platform() {
    let mut app = test_app();
    assert!(app.selected_session.is_none());

    let (tx, mut rx) = mpsc::unbounded_channel::<ComposerRequest>();
    app.set_composer_tx(tx);
    app.composer.input = "test message".into();

    app.submit_composer();

    let request = rx.try_recv().unwrap();
    assert_eq!(request.session_key.channel_id, "local");
    assert!(matches!(
        request.session_key.platform,
        Platform::Custom(ref name) if name == "tui"
    ));
}
