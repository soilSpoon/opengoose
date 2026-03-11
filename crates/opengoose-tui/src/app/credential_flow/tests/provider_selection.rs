use opengoose_provider_bridge::ConfigKeySummary;

use super::support::{api_key, make_provider, test_app_with_store};
use super::*;

#[test]
fn test_populate_provider_select_configure_filters_empty_keys() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers = vec![
        make_provider("openai", "OpenAI", vec![api_key("OPENAI_API_KEY")]),
        make_provider("local", "Local", vec![]),
    ];
    app.provider_select.purpose = ProviderSelectPurpose::Configure;
    app.populate_provider_select_from_cache();

    assert_eq!(app.provider_select.providers.len(), 1);
    assert_eq!(app.provider_select.providers[0], "OpenAI");
    assert_eq!(app.provider_select.provider_ids[0], "openai");
    assert!(app.provider_select.visible);
    assert_eq!(app.provider_select.selected, 0);
}

#[test]
fn test_populate_provider_select_list_models_shows_all() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers = vec![
        make_provider("openai", "OpenAI", vec![api_key("OPENAI_API_KEY")]),
        make_provider("local", "Local", vec![]),
    ];
    app.provider_select.purpose = ProviderSelectPurpose::ListModels;
    app.populate_provider_select_from_cache();

    assert_eq!(app.provider_select.providers.len(), 2);
}

#[test]
fn test_populate_provider_select_oauth_label() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers = vec![make_provider(
        "google",
        "Google",
        vec![ConfigKeySummary {
            name: "GOOGLE_TOKEN".into(),
            required: true,
            secret: true,
            oauth_flow: true,
            default: None,
            primary: true,
        }],
    )];
    app.provider_select.purpose = ProviderSelectPurpose::Configure;
    app.populate_provider_select_from_cache();

    assert_eq!(app.provider_select.providers[0], "Google (OAuth)");
}

#[test]
fn test_open_provider_select_sets_purpose() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers = vec![make_provider("openai", "OpenAI", vec![api_key("KEY")])];
    app.open_provider_select();

    assert_eq!(
        app.provider_select.purpose,
        ProviderSelectPurpose::Configure
    );
    assert!(app.provider_select.visible);
}

#[tokio::test]
async fn test_open_provider_select_for_configure_without_cached_providers_starts_loading() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers.clear();

    app.open_provider_select_for(ProviderSelectPurpose::Configure);

    assert_eq!(
        app.provider_select.purpose,
        ProviderSelectPurpose::Configure
    );
    // Modal is not shown until providers finish loading
    assert!(!app.provider_select.visible);
    assert!(app.provider_loading_rx.is_some());
}

#[test]
fn test_confirm_provider_select_configure_starts_flow() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers = vec![make_provider(
        "openai",
        "OpenAI",
        vec![api_key("OPENAI_API_KEY")],
    )];
    app.provider_select.purpose = ProviderSelectPurpose::Configure;
    app.provider_select.provider_ids = vec!["openai".into()];
    app.provider_select.selected = 0;
    app.provider_select.visible = true;

    app.confirm_provider_select();

    assert!(!app.provider_select.visible);
    assert_eq!(app.credential_flow.provider_id.as_deref(), Some("openai"));
    assert!(app.secret_input.visible);
    assert_eq!(
        app.secret_input.title.as_deref(),
        Some("OpenAI — API Key [OPENAI_API_KEY]")
    );
}

// ── confirm_provider_select tests ────────────────────────────────────────────

#[tokio::test]
async fn test_confirm_provider_select_list_models_hides_modal_and_starts_loading() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers = vec![make_provider("openai", "OpenAI", vec![api_key("KEY")])];
    app.provider_select.purpose = ProviderSelectPurpose::ListModels;
    app.provider_select.provider_ids = vec!["openai".into()];
    app.provider_select.selected = 0;
    app.provider_select.visible = true;

    app.confirm_provider_select();

    assert!(!app.provider_select.visible);
    assert!(app.model_select.visible);
    assert!(app.model_select.loading);
    assert_eq!(app.model_select.provider_name, "openai");
}

#[test]
fn test_confirm_provider_select_list_models_empty_ids_does_nothing() {
    let (mut app, _, _dir) = test_app_with_store();
    app.provider_select.purpose = ProviderSelectPurpose::ListModels;
    app.provider_select.provider_ids = vec![];
    app.provider_select.selected = 0;
    app.provider_select.visible = true;

    // No id at index 0, so modal stays visible and model_select unchanged
    app.confirm_provider_select();

    assert!(app.provider_select.visible);
    assert!(!app.model_select.visible);
}

#[tokio::test]
async fn test_fetch_models_initializes_model_select_state() {
    let (mut app, _, _dir) = test_app_with_store();

    app.fetch_models("anthropic");

    assert!(app.model_select.visible);
    assert!(app.model_select.loading);
    assert_eq!(app.model_select.provider_name, "anthropic");
    assert!(app.model_select.models.is_empty());
    assert_eq!(app.model_select.selected, 0);
    assert!(app.model_loading_rx.is_some());
}

#[test]
fn test_open_provider_select_for_list_models_includes_all_providers() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers = vec![
        make_provider("openai", "OpenAI", vec![api_key("KEY")]),
        make_provider("local", "Local", vec![]),
    ];

    app.open_provider_select_for(ProviderSelectPurpose::ListModels);

    assert_eq!(
        app.provider_select.purpose,
        ProviderSelectPurpose::ListModels
    );
    assert!(app.provider_select.visible);
    assert_eq!(app.provider_select.providers.len(), 2);
}
