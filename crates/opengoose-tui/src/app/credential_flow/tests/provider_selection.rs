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
