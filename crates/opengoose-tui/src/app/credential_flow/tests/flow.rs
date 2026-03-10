use opengoose_provider_bridge::ConfigKeySummary;

use super::support::{api_key, make_provider, test_app_with_store};
use super::*;

#[test]
fn test_start_credential_flow_sets_keys() {
    let (mut app, _, _dir) = test_app_with_store();
    app.cached_providers = vec![make_provider(
        "openai",
        "OpenAI",
        vec![api_key("OPENAI_API_KEY")],
    )];
    app.provider_select.provider_ids = vec!["openai".into()];
    app.provider_select.selected = 0;

    app.start_credential_flow();

    assert_eq!(app.credential_flow.provider_id, Some("openai".into()));
    assert_eq!(app.credential_flow.provider_display, Some("OpenAI".into()));
    assert_eq!(app.credential_flow.keys.len(), 1);
    assert_eq!(app.credential_flow.keys[0].env_var, "OPENAI_API_KEY");
    assert_eq!(app.credential_flow.keys[0].label, "API Key");
    assert!(app.secret_input.visible);
}

#[test]
fn test_start_credential_flow_key_label_mapping() {
    let (mut app, _, _dir) = test_app_with_store();
    let keys = vec![
        ConfigKeySummary {
            name: "MY_API_KEY".into(),
            required: true,
            secret: true,
            oauth_flow: false,
            default: None,
            primary: true,
        },
        ConfigKeySummary {
            name: "MY_TOKEN".into(),
            required: true,
            secret: true,
            oauth_flow: false,
            default: None,
            primary: true,
        },
        ConfigKeySummary {
            name: "HOST_URL".into(),
            required: false,
            secret: false,
            oauth_flow: false,
            default: Some("http://localhost".into()),
            primary: false,
        },
        ConfigKeySummary {
            name: "SOME_SETTING".into(),
            required: false,
            secret: false,
            oauth_flow: false,
            default: None,
            primary: false,
        },
    ];
    app.cached_providers = vec![make_provider("test", "Test", keys)];
    app.provider_select.provider_ids = vec!["test".into()];
    app.provider_select.selected = 0;

    app.start_credential_flow();

    assert_eq!(app.credential_flow.keys[0].label, "API Key");
    assert_eq!(app.credential_flow.keys[1].label, "Token");
    assert_eq!(app.credential_flow.keys[2].label, "URL");
    assert_eq!(app.credential_flow.keys[3].label, "Value");
}

#[test]
fn test_start_credential_flow_no_provider_ids() {
    let (mut app, _, _dir) = test_app_with_store();
    app.provider_select.provider_ids = vec![];
    app.provider_select.selected = 0;

    app.start_credential_flow();

    assert!(app.credential_flow.provider_id.is_none());
}

#[test]
fn test_save_credential_empty_required() {
    let (mut app, _, _dir) = test_app_with_store();
    app.credential_flow.provider_id = Some("test".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "API_KEY".into(),
        label: "Key".into(),
        secret: true,
        oauth_flow: false,
        required: true,
        default: None,
    });
    app.secret_input.input.clear();

    let result = app.save_credential_and_advance();
    assert!(result.is_ok());
    assert_eq!(
        app.secret_input.status_message,
        Some("Value cannot be empty".into())
    );
}

#[test]
fn test_save_credential_no_current_key() {
    let (mut app, _, _dir) = test_app_with_store();
    let result = app.save_credential_and_advance();
    assert!(result.is_ok());
}

#[test]
fn test_open_credential_input_optional_hint() {
    let (mut app, _, _dir) = test_app_with_store();
    app.credential_flow.provider_display = Some("Test".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "OPTIONAL_KEY".into(),
        label: "Value".into(),
        secret: false,
        oauth_flow: false,
        required: false,
        default: None,
    });

    app.advance_credential_flow();

    let title = app.secret_input.title.as_deref().unwrap();
    assert!(title.contains("(optional)"));
    assert!(!app.secret_input.is_secret);
}

#[test]
fn test_open_credential_input_default_hint() {
    let (mut app, _, _dir) = test_app_with_store();
    app.credential_flow.provider_display = Some("Test".into());
    app.credential_flow.keys.push(CredentialKey {
        env_var: "HOST".into(),
        label: "URL".into(),
        secret: false,
        oauth_flow: false,
        required: true,
        default: Some("http://localhost".into()),
    });

    app.advance_credential_flow();

    let title = app.secret_input.title.as_deref().unwrap();
    assert!(title.contains("(Enter for default)"));
}
