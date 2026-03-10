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
