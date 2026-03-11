use std::sync::Arc;

use opengoose_secrets::SecretStore;
use tokio::sync::oneshot;

use super::super::state::*;
use super::tests_support::{MockStore, test_app, test_app_with_store};

#[test]
fn test_tick_provider_loading_closed() {
    let mut app = test_app();
    let (tx, rx) = oneshot::channel::<Vec<opengoose_provider_bridge::ProviderSummary>>();
    drop(tx);
    app.provider_loading_rx = Some(rx);

    app.tick();

    assert!(app.provider_loading_rx.is_none());
    assert!(!app.provider_select.visible);
    assert_eq!(app.events.back().unwrap().level, EventLevel::Error);
}

#[test]
fn test_tick_model_loading_success() {
    let mut app = test_app();
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(vec!["gpt-4".to_string(), "gpt-3.5".to_string()]);
    app.model_loading_rx = Some(rx);
    app.model_select.loading = true;

    app.tick();

    assert!(app.model_loading_rx.is_none());
    assert!(!app.model_select.loading);
    assert_eq!(app.model_select.models.len(), 2);
}

#[test]
fn test_tick_provider_loading_success() {
    let mut app = test_app();
    let (tx, rx) = oneshot::channel();
    let providers = vec![opengoose_provider_bridge::ProviderSummary {
        name: "openai".into(),
        display_name: "OpenAI".into(),
        description: "desc".into(),
        default_model: "gpt-4".into(),
        known_models: vec![],
        config_keys: vec![opengoose_provider_bridge::ConfigKeySummary {
            name: "OPENAI_API_KEY".into(),
            required: true,
            secret: true,
            oauth_flow: false,
            default: None,
            primary: true,
        }],
    }];
    let _ = tx.send(providers);
    app.provider_loading_rx = Some(rx);

    app.tick();

    assert!(app.provider_loading_rx.is_none());
    assert_eq!(app.cached_providers.len(), 1);
    assert!(app.provider_select.visible);
}

#[test]
fn test_tick_model_loading_closed() {
    let mut app = test_app();
    let (tx, rx) = oneshot::channel::<Vec<String>>();
    drop(tx);
    app.model_loading_rx = Some(rx);
    app.model_select.loading = true;

    app.tick();

    assert!(app.model_loading_rx.is_none());
    assert!(!app.model_select.loading);
    assert_eq!(app.events.back().unwrap().level, EventLevel::Error);
}

#[test]
fn test_oauth_failure_surfaces_notice() {
    let store = Arc::new(MockStore::new());
    let mut app = test_app_with_store(store);
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(Err(anyhow::anyhow!("auth failed")));
    app.oauth_done_rx = Some(rx);

    app.tick();

    assert!(app.status_notice.is_some());
    assert!(
        app.status_notice
            .as_ref()
            .unwrap()
            .message
            .contains("OAuth failed")
    );
}

#[test]
fn test_oauth_success_with_more_keys_advances_credential_flow() {
    let mut app = test_app();
    app.credential_flow.provider_id = Some("provider".into());
    app.credential_flow.provider_display = Some("Test Provider".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "OAUTH_TOKEN".into(),
        label: "OAuth".into(),
        secret: true,
        oauth_flow: true,
        required: true,
        default: None,
    });
    app.credential_flow.keys.push(CredentialKey {
        env_var: "TEST_KEY".into(),
        label: "API Key".into(),
        secret: true,
        oauth_flow: false,
        required: true,
        default: None,
    });
    let (tx, rx) = oneshot::channel();
    let _ = tx.send(Ok(()));
    app.oauth_done_rx = Some(rx);

    app.tick();

    assert_eq!(app.credential_flow.current_key, 1);
    assert!(app.secret_input.visible);
    assert_eq!(
        app.secret_input.title.as_deref(),
        Some("Test Provider — API Key [TEST_KEY]")
    );
}

#[test]
fn test_oauth_success_without_more_keys_stores_credentials_and_resets_flow() {
    let store = Arc::new(MockStore::new());
    let mut app = test_app_with_store(store.clone());
    app.credential_flow.provider_id = Some("provider".into());
    app.credential_flow.provider_display = Some("Provider".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "PROVIDER_TOKEN".into(),
        label: "Token".into(),
        secret: true,
        oauth_flow: false,
        required: true,
        default: None,
    });
    app.credential_flow
        .collected
        .push(("PROVIDER_TOKEN".into(), "abc123".into()));

    let (tx, rx) = oneshot::channel();
    let _ = tx.send(Ok(()));
    app.oauth_done_rx = Some(rx);

    app.tick();

    assert_eq!(
        store.get("provider_token").unwrap().unwrap().as_str(),
        "abc123"
    );
    assert_eq!(
        app.credential_flow.provider_id, None,
        "Flow state should reset after successful credential storage"
    );
    assert_eq!(
        app.events.back().unwrap().summary,
        "Authenticated with Provider."
    );
}

#[test]
fn test_oauth_closed_channel_surfaces_error_notice_and_resets_flow() {
    let mut app = test_app();
    let (tx, rx) = oneshot::channel();
    drop(tx);
    app.oauth_done_rx = Some(rx);

    app.tick();

    assert!(app.status_notice.is_some());
    assert_eq!(app.status_notice.as_ref().unwrap().level, EventLevel::Error);
    assert!(
        app.status_notice
            .as_ref()
            .unwrap()
            .message
            .contains("OAuth task terminated unexpectedly")
    );
}
