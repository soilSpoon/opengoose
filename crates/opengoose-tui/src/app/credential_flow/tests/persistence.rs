use tokio::sync::oneshot;

use crate::app::state::AppMode;

use super::support::test_app_with_store;
use super::*;

#[test]
fn test_save_credential_uses_default_when_empty() {
    let (mut app, store, _dir) = test_app_with_store();
    app.credential_flow.provider_id = Some("test".into());
    app.credential_flow.provider_display = Some("Test".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "HOST".into(),
        label: "URL".into(),
        secret: false,
        oauth_flow: false,
        required: true,
        default: Some("http://localhost:8080".into()),
    });
    app.secret_input.input.clear();

    let result = app.save_credential_and_advance();
    assert!(result.is_ok());

    assert_eq!(
        store.secrets.lock().unwrap().get("host"),
        Some(&"http://localhost:8080".into())
    );
}

#[test]
fn test_save_credential_with_value() {
    let (mut app, store, _dir) = test_app_with_store();
    app.credential_flow.provider_id = Some("test".into());
    app.credential_flow.provider_display = Some("Test".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "API_KEY".into(),
        label: "Key".into(),
        secret: true,
        oauth_flow: false,
        required: true,
        default: None,
    });
    app.secret_input.input = "sk-12345".into();

    let result = app.save_credential_and_advance();
    assert!(result.is_ok());

    assert_eq!(
        store.secrets.lock().unwrap().get("api_key"),
        Some(&"sk-12345".into())
    );
}

#[test]
fn test_store_credentials_no_provider() {
    let (mut app, _, _dir) = test_app_with_store();
    app.credential_flow.provider_id = None;

    let result = app.store_credentials();
    assert!(result.is_ok());
}

#[test]
fn test_store_credentials_resets_ui() {
    let (mut app, _, _dir) = test_app_with_store();
    app.credential_flow.provider_id = Some("openai".into());
    app.credential_flow.provider_display = Some("OpenAI".into());
    app.credential_flow
        .collected
        .push(("OPENAI_API_KEY".into(), "sk-key".into()));
    app.secret_input.visible = true;
    app.secret_input.title = Some("title".into());

    let result = app.store_credentials();
    assert!(result.is_ok());

    assert!(!app.secret_input.visible);
    assert!(app.secret_input.input.is_empty());
    assert!(app.secret_input.status_message.is_none());
    assert!(app.secret_input.title.is_none());
    assert!(app.secret_input.is_secret);
    assert!(app.credential_flow.provider_id.is_none());
    assert!(app.events.back().unwrap().summary.contains("Authenticated"));
}

#[test]
fn test_save_credential_and_advance_through_multiple_keys() {
    let (mut app, store, _dir) = test_app_with_store();
    app.credential_flow.provider_id = Some("openai".into());
    app.credential_flow.provider_display = Some("OpenAI".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "OPENAI_API_KEY".into(),
        label: "API Key".into(),
        secret: true,
        oauth_flow: false,
        required: true,
        default: None,
    });
    app.credential_flow.keys.push(CredentialKey {
        env_var: "OPENAI_BASE_URL".into(),
        label: "URL".into(),
        secret: false,
        oauth_flow: false,
        required: false,
        default: Some("https://api.openai.com".into()),
    });

    app.secret_input.input = "sk-12345".into();
    let result = app.save_credential_and_advance();
    assert!(result.is_ok());
    assert_eq!(app.credential_flow.current_key, 1);
    assert_eq!(app.credential_flow.collected.len(), 1);
    assert!(app.secret_input.visible);

    app.secret_input.input = "".into();
    let result = app.save_credential_and_advance();
    assert!(result.is_ok());

    assert!(app.credential_flow.provider_id.is_none());
    assert!(app.credential_flow.collected.is_empty());
    assert!(!app.secret_input.visible);
    let secrets = store.secrets.lock().unwrap();
    assert_eq!(secrets.get("openai_api_key"), Some(&"sk-12345".into()));
    assert_eq!(
        secrets.get("openai_base_url"),
        Some(&"https://api.openai.com".into())
    );
    assert!(
        app.events
            .back()
            .unwrap()
            .summary
            .contains("Authenticated with OpenAI.")
    );
}

#[test]
fn test_save_credential_optional_skip() {
    let (mut app, _, _dir) = test_app_with_store();
    app.credential_flow.provider_id = Some("test".into());
    app.credential_flow.provider_display = Some("Test".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "OPTIONAL".into(),
        label: "Value".into(),
        secret: false,
        oauth_flow: false,
        required: false,
        default: None,
    });
    app.secret_input.input.clear();

    let result = app.save_credential_and_advance();
    assert!(result.is_ok());
    assert!(app.credential_flow.collected.is_empty());
}

// ── save_secret_and_notify tests ─────────────────────────────────────────────

#[test]
fn test_save_secret_and_notify_empty_token_sets_status_message() {
    let (mut app, _, _dir) = test_app_with_store();
    app.secret_input.input.clear();

    let result = app.save_secret_and_notify();
    assert!(result.is_ok());
    assert_eq!(
        app.secret_input.status_message.as_deref(),
        Some("Token cannot be empty")
    );
}

#[test]
fn test_save_secret_and_notify_saves_token_to_store() {
    let (mut app, store, _dir) = test_app_with_store();
    app.secret_input.input = "my-discord-token".into();

    let result = app.save_secret_and_notify();
    assert!(result.is_ok());

    let secrets = store.secrets.lock().unwrap();
    let saved = secrets.get("discord_bot_token");
    assert_eq!(saved.map(|s| s.as_str()), Some("my-discord-token"));
}

#[test]
fn test_save_secret_and_notify_clears_input_when_saved() {
    let (mut app, _, _dir) = test_app_with_store();
    app.secret_input.input = "tok".into();

    let result = app.save_secret_and_notify();
    assert!(result.is_ok());
    assert!(app.secret_input.input.is_empty());
    assert!(!app.secret_input.visible);
    assert!(app.secret_input.status_message.is_none());
}

#[test]
fn test_save_secret_and_notify_with_sender_sets_normal_mode() {
    let (mut app, _, _dir) = test_app_with_store();
    let (tx, mut rx) = oneshot::channel();
    app.token_sender = Some(tx);
    app.secret_input.input = "tok-abc".into();

    let result = app.save_secret_and_notify();
    assert!(result.is_ok());
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.token_sender.is_none());

    // The token was sent over the channel
    let received = rx.try_recv().unwrap();
    assert_eq!(received, "tok-abc");
}

#[test]
fn test_save_secret_and_notify_without_sender_pushes_event() {
    let (mut app, _, _dir) = test_app_with_store();
    app.token_sender = None;
    app.secret_input.input = "tok-xyz".into();

    let result = app.save_secret_and_notify();
    assert!(result.is_ok());

    let last_event = app.events.back().unwrap();
    assert!(last_event.summary.contains("Token updated"));
}
