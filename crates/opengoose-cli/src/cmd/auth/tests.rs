use super::register;
use super::*;

use std::sync::{Mutex, Once};

use opengoose_provider_bridge::{ConfigKeySummary, ProviderSummary};
use opengoose_secrets::{ConfigFile, ProviderMeta};

static ENV_LOCK: Mutex<()> = Mutex::new(());
static RUSTLS_INIT: Once = Once::new();

fn ensure_rustls_provider() {
    RUSTLS_INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

struct EnvVarGuard {
    name: String,
    original: Option<String>,
}

impl EnvVarGuard {
    fn set(name: &str, value: Option<&str>) -> Self {
        let original = std::env::var(name).ok();
        // Safety: test-only helper guarded by ENV_LOCK.
        unsafe {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        Self {
            name: name.to_string(),
            original,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // Safety: test-only helper guarded by ENV_LOCK.
        unsafe {
            match &self.original {
                Some(value) => std::env::set_var(&self.name, value),
                None => std::env::remove_var(&self.name),
            }
        }
    }
}

fn with_env_var<T>(name: &str, value: Option<&str>, test: impl FnOnce() -> T) -> T {
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvVarGuard::set(name, value);
    test()
}

#[test]
fn key_label_matches_expected_hints() {
    let api_key = ConfigKeySummary {
        name: "OPENAI_API_KEY".into(),
        required: true,
        secret: true,
        oauth_flow: false,
        default: None,
        primary: true,
    };
    let token = ConfigKeySummary {
        name: "SLACK_APP_TOKEN".into(),
        required: true,
        secret: true,
        oauth_flow: false,
        default: None,
        primary: true,
    };
    let location = ConfigKeySummary {
        name: "AWS_LOCATION".into(),
        required: false,
        secret: false,
        oauth_flow: false,
        default: None,
        primary: false,
    };
    let profile = ConfigKeySummary {
        name: "AWS_PROFILE".into(),
        required: false,
        secret: false,
        oauth_flow: false,
        default: None,
        primary: false,
    };
    let project = ConfigKeySummary {
        name: "GOOGLE_PROJECT".into(),
        required: false,
        secret: false,
        oauth_flow: false,
        default: None,
        primary: false,
    };
    let deployment = ConfigKeySummary {
        name: "AZURE_DEPLOYMENT".into(),
        required: false,
        secret: false,
        oauth_flow: false,
        default: None,
        primary: false,
    };
    let fallback = ConfigKeySummary {
        name: "CUSTOM_SETTING".into(),
        required: false,
        secret: false,
        oauth_flow: false,
        default: None,
        primary: false,
    };

    assert_eq!(register::key_label(&api_key), "API Key");
    assert_eq!(register::key_label(&token), "Token");
    assert_eq!(register::key_label(&location), "Location");
    assert_eq!(register::key_label(&profile), "Profile");
    assert_eq!(register::key_label(&project), "Project ID");
    assert_eq!(register::key_label(&deployment), "Deployment");
    assert_eq!(register::key_label(&fallback), "Value");
}

#[tokio::test]
async fn execute_list_succeeds() {
    ensure_rustls_provider();
    execute(
        AuthAction::List,
        CliOutput::new(crate::cmd::output::OutputMode::Text),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn execute_models_reports_unknown_provider() {
    ensure_rustls_provider();

    let err = execute(
        AuthAction::Models {
            provider: "definitely-unknown-provider".into(),
        },
        CliOutput::new(crate::cmd::output::OutputMode::Text),
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("Unknown provider: definitely-unknown-provider")
    );
}

#[tokio::test]
async fn execute_login_reports_unknown_provider() {
    ensure_rustls_provider();

    let err = execute(
        AuthAction::Login {
            provider: Some("definitely-unknown-provider".into()),
        },
        CliOutput::new(crate::cmd::output::OutputMode::Text),
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("unknown provider `definitely-unknown-provider`")
    );
}

fn make_key_with_primary(
    name: &str,
    required: bool,
    oauth_flow: bool,
    primary: bool,
) -> ConfigKeySummary {
    ConfigKeySummary {
        name: name.into(),
        required,
        secret: true,
        oauth_flow,
        default: None,
        primary,
    }
}

fn make_key(name: &str, required: bool, oauth_flow: bool) -> ConfigKeySummary {
    make_key_with_primary(name, required, oauth_flow, true)
}

fn make_provider(name: &str, keys: Vec<ConfigKeySummary>) -> ProviderSummary {
    ProviderSummary {
        name: name.into(),
        display_name: name.into(),
        description: String::new(),
        default_model: String::new(),
        known_models: vec![],
        config_keys: keys,
    }
}

fn config_with_provider_keys(provider_name: &str, keys_in_keyring: &[&str]) -> ConfigFile {
    let mut config = ConfigFile::default();
    config.providers.insert(
        provider_name.to_string(),
        ProviderMeta {
            keys_in_keyring: keys_in_keyring.iter().map(|key| key.to_string()).collect(),
        },
    );
    config
}

#[test]
fn provider_auth_type_oauth() {
    let provider = make_provider("google", vec![make_key("GOOGLE_TOKEN", true, true)]);
    assert_eq!(provider_auth_type(&provider), "oauth");
}

#[test]
fn provider_auth_type_key() {
    let provider = make_provider("openai", vec![make_key("OPENAI_API_KEY", true, false)]);
    assert_eq!(provider_auth_type(&provider), "key");
}

#[test]
fn provider_auth_type_none_when_no_keys() {
    let provider = make_provider("local", vec![]);
    assert_eq!(provider_auth_type(&provider), "none");
}

#[test]
fn provider_auth_type_prefers_primary_key_over_first_key() {
    let provider = make_provider(
        "mixed",
        vec![
            make_key_with_primary("MIXED_API_KEY", true, false, false),
            make_key_with_primary("MIXED_TOKEN", true, true, true),
        ],
    );
    assert_eq!(provider_auth_type(&provider), "oauth");
}

#[test]
fn provider_status_ready_when_no_required_keys() {
    let provider = make_provider("local", vec![]);
    let config = ConfigFile::default();
    let (status, via) = provider_status(&provider, &config);
    assert_eq!(status, "ready");
    assert!(via.is_none());
}

#[test]
fn provider_status_not_configured_when_key_missing() {
    let provider = make_provider(
        "test-provider-missing",
        vec![make_key("OPENGOOSE_TEST_MISSING_KEY_12345", true, false)],
    );
    with_env_var("OPENGOOSE_TEST_MISSING_KEY_12345", None, || {
        let config = ConfigFile::default();
        let (status, via) = provider_status(&provider, &config);
        assert_eq!(status, "not configured");
        assert!(via.is_none());
    });
}

#[test]
fn provider_status_configured_via_env_when_key_set() {
    let provider = make_provider(
        "test-provider-env",
        vec![make_key("OPENGOOSE_TEST_ENV_KEY_12345", true, false)],
    );
    with_env_var("OPENGOOSE_TEST_ENV_KEY_12345", Some("test-value"), || {
        let config = ConfigFile::default();
        let (status, via) = provider_status(&provider, &config);
        assert_eq!(status, "configured");
        assert_eq!(via, Some("env"));
    });
}

#[test]
fn provider_status_not_configured_when_env_value_is_empty() {
    let provider = make_provider(
        "test-provider-empty",
        vec![make_key("OPENGOOSE_TEST_EMPTY_KEY_12345", true, false)],
    );
    with_env_var("OPENGOOSE_TEST_EMPTY_KEY_12345", Some(""), || {
        let config = ConfigFile::default();
        let (status, via) = provider_status(&provider, &config);
        assert_eq!(status, "not configured");
        assert!(via.is_none());
    });
}

#[test]
fn provider_status_configured_via_keyring_when_all_required_keys_exist() {
    let provider = make_provider(
        "keyring-provider",
        vec![
            make_key("KEYRING_API_KEY", true, false),
            make_key_with_primary("KEYRING_ORG_ID", true, false, false),
        ],
    );
    let config =
        config_with_provider_keys("keyring-provider", &["keyring_api_key", "keyring_org_id"]);
    let (status, via) = provider_status(&provider, &config);
    assert_eq!(status, "configured");
    assert_eq!(via, Some("keyring"));
}

#[test]
fn provider_status_not_configured_when_keyring_is_missing_required_key() {
    let provider = make_provider(
        "partial-keyring-provider",
        vec![
            make_key("PARTIAL_API_KEY", true, false),
            make_key_with_primary("PARTIAL_ORG_ID", true, false, false),
        ],
    );
    let config = config_with_provider_keys("partial-keyring-provider", &["partial_api_key"]);
    let (status, via) = provider_status(&provider, &config);
    assert_eq!(status, "not configured");
    assert!(via.is_none());
}

#[test]
fn provider_status_prefers_env_when_env_and_keyring_are_both_available() {
    let provider = make_provider(
        "env-precedence-provider",
        vec![make_key("OPENGOOSE_TEST_PRECEDENCE_KEY_12345", true, false)],
    );
    with_env_var(
        "OPENGOOSE_TEST_PRECEDENCE_KEY_12345",
        Some("present"),
        || {
            let config = config_with_provider_keys(
                "env-precedence-provider",
                &["opengoose_test_precedence_key_12345"],
            );
            let (status, via) = provider_status(&provider, &config);
            assert_eq!(status, "configured");
            assert_eq!(via, Some("env"));
        },
    );
}
