use std::sync::Arc;

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
