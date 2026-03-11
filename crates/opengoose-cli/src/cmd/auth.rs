use std::io::Write;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_provider_bridge::{ConfigKeySummary, GooseProviderService, ProviderSummary};
use opengoose_secrets::{ConfigFile, KeyringBackend, SecretKey, SecretStore};

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose auth list\n  opengoose auth login openai\n  opengoose --json auth models anthropic"
)]
/// Subcommands for `opengoose auth`.
pub enum AuthAction {
    /// Authenticate with an AI provider (supports OAuth and API key)
    #[command(after_help = "Example:\n  opengoose auth login openai")]
    Login {
        /// Provider name (e.g. anthropic, openai). Interactive if omitted.
        provider: Option<String>,
    },
    /// Remove stored credentials for a provider
    #[command(after_help = "Example:\n  opengoose auth logout openai")]
    Logout {
        /// Provider name (e.g. anthropic, openai)
        provider: String,
    },
    /// List all providers and their authentication status
    #[command(after_help = "Examples:\n  opengoose auth list\n  opengoose --json auth ls")]
    #[command(alias = "ls")]
    List,
    /// List available models for a provider
    #[command(
        after_help = "Examples:\n  opengoose auth models openai\n  opengoose --json auth models anthropic"
    )]
    Models {
        /// Provider name (e.g. anthropic, openai)
        provider: String,
    },
    /// Store a custom secret in the OS keyring (e.g. discord_bot_token)
    #[command(after_help = "Example:\n  opengoose auth set discord_bot_token")]
    Set {
        /// Secret key name
        key: String,
    },
    /// Remove a custom secret from the OS keyring
    #[command(after_help = "Example:\n  opengoose auth remove discord_bot_token")]
    Remove {
        /// Secret key name
        key: String,
    },
}

/// Dispatch and execute the selected auth subcommand.
pub async fn execute(action: AuthAction, output: CliOutput) -> Result<()> {
    match action {
        AuthAction::Login { provider } => cmd_login(provider.as_deref(), output).await,
        AuthAction::Logout { provider } => cmd_logout(&provider, output).await,
        AuthAction::List => cmd_list(output).await,
        AuthAction::Models { provider } => cmd_models(&provider, output).await,
        AuthAction::Set { key } => cmd_set(&key, output),
        AuthAction::Remove { key } => cmd_remove(&key, output),
    }
}

async fn cmd_login(provider_arg: Option<&str>, output: CliOutput) -> Result<()> {
    let providers = GooseProviderService::list_providers().await;

    let provider = match provider_arg {
        Some(id) => match providers.iter().find(|p| p.name == id) {
            Some(p) => p,
            None => bail!("unknown provider `{id}`"),
        },
        None => prompt_provider_selection(&providers)?,
    };

    if provider.config_keys.is_empty() {
        if output.is_json() {
            output.print_json(&json!({
                "ok": true,
                "command": "auth.login",
                "provider": provider.name,
                "display_name": provider.display_name,
                "status": "ready",
            }))?;
        } else {
            println!(
                "{} does not require authentication. Just set it as your provider.",
                provider.display_name
            );
        }
        return Ok(());
    }

    let has_oauth = provider.config_keys.iter().any(|k| k.oauth_flow);

    if !output.is_json() {
        if has_oauth {
            println!(
                "Configuring {} (OAuth + credentials)",
                provider.display_name
            );
        } else {
            println!("Configuring {} (credentials needed)", provider.display_name);
        }
    }

    for key in provider.config_keys.iter().filter(|k| k.oauth_flow) {
        if !output.is_json() {
            println!(
                "Starting OAuth authentication for {} ({})...",
                provider.display_name, key.name
            );
        }
        GooseProviderService::run_oauth(&provider.name).await?;
        if !output.is_json() {
            println!("OAuth authentication completed.");
        }
    }

    let mut collected: Vec<(String, String)> = Vec::new();
    for key in provider.config_keys.iter().filter(|k| !k.oauth_flow) {
        if !key.required {
            eprint!("  Configure {} (optional)? [y/N]: ", key.name);
            std::io::stderr().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                continue;
            }
        }

        let value = if key.secret {
            rpassword::prompt_password(format!("  {} [{}]: ", key_label(key), key.name))?
        } else {
            prompt_text_input(key)?
        };

        if value.is_empty() {
            if key.required {
                bail!("empty value for {} — aborting", key.name);
            }
            continue;
        }

        collected.push((key.name.clone(), value));
    }

    // Store all credentials only after successful collection
    for (env_var, value) in &collected {
        GooseProviderService::store_credential(&provider.name, env_var, value)?;
    }

    let models = match GooseProviderService::fetch_models(&provider.name).await {
        Ok(models) if !models.is_empty() => Some(models),
        Ok(_) => None,
        Err(err) => {
            tracing::debug!("Could not fetch models after login: {err}");
            None
        }
    };

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.login",
            "provider": provider.name,
            "display_name": provider.display_name,
            "configured_keys": collected.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>(),
            "models": models,
        }))?;
        return Ok(());
    }

    println!("Authenticated with {}.", provider.display_name);
    if let Some(models) = models {
        println!();
        println!("Available models ({}):", models.len());
        for (i, model) in models.iter().take(10).enumerate() {
            println!("  {:>2}. {}", i + 1, model);
        }
        if models.len() > 10 {
            println!(
                "  ... and {} more (use `opengoose auth models {}` to see all)",
                models.len() - 10,
                provider.name
            );
        }
    }

    Ok(())
}

async fn cmd_logout(provider_id: &str, output: CliOutput) -> Result<()> {
    let providers = GooseProviderService::list_providers().await;
    let mut config = ConfigFile::load()?;

    let mut keys_to_delete = std::collections::BTreeSet::new();
    if let Some(meta) = config.providers.get(provider_id) {
        for k in &meta.keys_in_keyring {
            keys_to_delete.insert(k.clone());
        }
    }
    if let Some(provider) = providers.iter().find(|p| p.name == provider_id) {
        for key in &provider.config_keys {
            keys_to_delete.insert(key.name.to_lowercase());
        }
    }
    if keys_to_delete.is_empty() && !config.providers.contains_key(provider_id) {
        bail!("unknown provider `{provider_id}` and no stored credentials found");
    }

    let mut errors = Vec::new();
    for keyring_key in &keys_to_delete {
        if let Err(e) = KeyringBackend.delete(keyring_key) {
            errors.push(format!("{keyring_key}: {e}"));
        }
    }

    if !errors.is_empty() {
        let display = providers
            .iter()
            .find(|p| p.name == provider_id)
            .map(|p| p.display_name.as_str())
            .unwrap_or(provider_id);
        bail!(
            "failed to remove some credentials for {display}: {}",
            errors.join("; ")
        );
    }

    config.remove_provider(provider_id);
    config.save()?;

    let display = providers
        .iter()
        .find(|p| p.name == provider_id)
        .map(|p| p.display_name.as_str())
        .unwrap_or(provider_id);

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.logout",
            "provider": provider_id,
            "display_name": display,
            "removed_keys": keys_to_delete,
        }))?;
    } else {
        println!("Logged out from {display}.");
    }

    Ok(())
}

async fn cmd_list(output: CliOutput) -> Result<()> {
    let providers = GooseProviderService::list_providers().await;
    let config = ConfigFile::load()?;

    if output.is_json() {
        let providers_json = providers
            .iter()
            .map(|provider| {
                let auth_type = provider_auth_type(provider);
                let (status, configured_via) = provider_status(provider, &config);
                json!({
                    "name": provider.name,
                    "display_name": provider.display_name,
                    "description": provider.description,
                    "default_model": provider.default_model,
                    "known_models": provider.known_models,
                    "auth": auth_type,
                    "status": status,
                    "configured_via": configured_via,
                })
            })
            .collect::<Vec<_>>();

        output.print_json(&json!({
            "ok": true,
            "command": "auth.list",
            "providers": providers_json,
            "custom_secrets_configured": !config.secrets.is_empty(),
        }))?;
        return Ok(());
    }

    println!("{}", output.heading("Providers"));
    let rows = providers
        .iter()
        .map(|provider| {
            let auth_type = provider_auth_type(provider);
            let (status, _configured_via) = provider_status(provider, &config);
            vec![
                provider.display_name.clone(),
                auth_type.to_string(),
                provider.default_model.clone(),
                status.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    print!(
        "{}",
        format_table(&["PROVIDER", "AUTH", "DEFAULT MODEL", "STATUS"], &rows)
    );

    if !config.secrets.is_empty() {
        println!();
        println!("Custom secrets: configured");
    }

    Ok(())
}

async fn cmd_models(provider_name: &str, output: CliOutput) -> Result<()> {
    let models = GooseProviderService::fetch_models(provider_name).await?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.models",
            "provider": provider_name,
            "models": models,
        }))?;
        return Ok(());
    }

    if models.is_empty() {
        println!("No models found (provider may not support model listing).");
    } else {
        println!("{}", output.heading(&format!("Models for {provider_name}")));
        let rows = models
            .iter()
            .map(|model| vec![model.clone()])
            .collect::<Vec<_>>();
        print!("{}", format_table(&["MODEL"], &rows));
    }

    Ok(())
}

fn cmd_set(key_name: &str, output: CliOutput) -> Result<()> {
    let key = SecretKey::from_str_canonical(key_name);

    let value = rpassword::prompt_password(format!("Enter value for `{key}`: "))?;
    if value.is_empty() {
        bail!("empty value — aborting");
    }

    KeyringBackend.set(key.as_str(), &value)?;

    let mut config = ConfigFile::load()?;
    config.mark_in_keyring(&key);
    config.save()?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.set",
            "key": key.as_str(),
            "stored": true,
        }))?;
    } else {
        println!("Stored `{key}` in OS keyring.");
    }

    Ok(())
}

fn cmd_remove(key_name: &str, output: CliOutput) -> Result<()> {
    let key = SecretKey::from_str_canonical(key_name);

    let deleted = KeyringBackend.delete(key.as_str())?;

    let mut config = ConfigFile::load()?;
    config.remove(&key);
    config.save()?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "auth.remove",
            "key": key.as_str(),
            "removed": deleted,
        }))?;
    } else if deleted {
        println!("Removed `{key}` from OS keyring.");
    } else {
        println!("`{key}` was not in the OS keyring (metadata cleared).");
    }
    Ok(())
}

fn prompt_provider_selection(providers: &[ProviderSummary]) -> Result<&ProviderSummary> {
    let items: Vec<_> = providers
        .iter()
        .filter(|p| !p.config_keys.is_empty())
        .collect();

    eprintln!("Select a provider:");
    for (i, p) in items.iter().enumerate() {
        let auth_hint = if p.config_keys.iter().any(|k| k.oauth_flow) {
            " (OAuth)"
        } else {
            ""
        };
        eprintln!("  [{:>2}] {}{auth_hint}", i + 1, p.display_name);
    }
    eprintln!();
    eprint!("Enter number: ");
    std::io::stderr().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    let idx: usize = input
        .trim()
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("invalid selection"))?;

    items
        .get(idx.wrapping_sub(1))
        .copied()
        .ok_or_else(|| anyhow::anyhow!("selection out of range (enter 1–{})", items.len()))
}

fn key_label(key: &ConfigKeySummary) -> &str {
    if key.name.ends_with("_API_KEY") || key.name.ends_with("_KEY") {
        "API Key"
    } else if key.name.ends_with("_TOKEN") {
        "Token"
    } else if key.name.contains("HOST") || key.name.contains("ENDPOINT") {
        "URL"
    } else if key.name.contains("REGION") {
        "Region"
    } else if key.name.contains("PROFILE") {
        "Profile"
    } else if key.name.contains("PROJECT") {
        "Project ID"
    } else if key.name.contains("LOCATION") {
        "Location"
    } else if key.name.contains("DEPLOYMENT") {
        "Deployment"
    } else {
        "Value"
    }
}

fn prompt_text_input(key: &ConfigKeySummary) -> Result<String> {
    let label = key_label(key);
    let prompt = match &key.default {
        Some(d) => format!("  {label} [{} (default: {d})]: ", key.name),
        None => format!("  {label} [{}]: ", key.name),
    };
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    if trimmed.is_empty()
        && let Some(d) = &key.default
    {
        return Ok(d.clone());
    }
    Ok(trimmed)
}

fn provider_auth_type(provider: &ProviderSummary) -> &'static str {
    let primary_key = provider
        .config_keys
        .iter()
        .find(|key| key.primary)
        .or_else(|| provider.config_keys.first());

    match primary_key {
        Some(key) if key.oauth_flow => "oauth",
        Some(_) => "key",
        None => "none",
    }
}

fn provider_status(
    provider: &ProviderSummary,
    config: &ConfigFile,
) -> (&'static str, Option<&'static str>) {
    let required_keys: Vec<_> = provider
        .config_keys
        .iter()
        .filter(|key| key.required)
        .collect();
    let keyring_keys = config
        .providers
        .get(&provider.name)
        .map(|metadata| &metadata.keys_in_keyring);

    let all_required_in_env = !required_keys.is_empty()
        && required_keys
            .iter()
            .all(|key| std::env::var(&key.name).is_ok_and(|value| !value.is_empty()));
    if all_required_in_env {
        return ("configured", Some("env"));
    }

    let all_required_in_keyring = !required_keys.is_empty()
        && keyring_keys.is_some_and(|keys| {
            required_keys
                .iter()
                .all(|key| keys.contains(&key.name.to_lowercase()))
        });
    if all_required_in_keyring {
        return ("configured", Some("keyring"));
    }

    if required_keys.is_empty() {
        ("ready", None)
    } else {
        ("not configured", None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Mutex, Once};

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

        assert_eq!(key_label(&api_key), "API Key");
        assert_eq!(key_label(&token), "Token");
        assert_eq!(key_label(&location), "Location");
        assert_eq!(key_label(&profile), "Profile");
        assert_eq!(key_label(&project), "Project ID");
        assert_eq!(key_label(&deployment), "Deployment");
        assert_eq!(key_label(&fallback), "Value");
    }

    #[test]
    fn key_label_host_and_endpoint_return_url() {
        let host = ConfigKeySummary {
            name: "OLLAMA_HOST".into(),
            required: false,
            secret: false,
            oauth_flow: false,
            default: None,
            primary: false,
        };
        let endpoint = ConfigKeySummary {
            name: "AZURE_ENDPOINT".into(),
            required: false,
            secret: false,
            oauth_flow: false,
            default: None,
            primary: false,
        };
        assert_eq!(key_label(&host), "URL");
        assert_eq!(key_label(&endpoint), "URL");
    }

    #[test]
    fn key_label_region_returns_region() {
        let region = ConfigKeySummary {
            name: "AWS_REGION".into(),
            required: false,
            secret: false,
            oauth_flow: false,
            default: None,
            primary: false,
        };
        assert_eq!(key_label(&region), "Region");
    }

    #[test]
    fn provider_status_optional_keys_do_not_affect_ready_status() {
        let provider = make_provider(
            "optional-keys-provider",
            vec![make_key_with_primary("OPTIONAL_SETTING", false, false, false)],
        );
        let config = opengoose_secrets::ConfigFile::default();
        let (status, via) = provider_status(&provider, &config);
        assert_eq!(status, "ready");
        assert!(via.is_none());
    }

    #[test]
    fn provider_status_env_key_not_counted_when_unrelated_provider() {
        // Setting an env var for a different provider key should not mark this provider configured
        let provider = make_provider(
            "isolated-provider",
            vec![make_key("ISOLATED_PROVIDER_API_KEY", true, false)],
        );
        with_env_var("OTHER_PROVIDER_API_KEY", Some("value"), || {
            let config = opengoose_secrets::ConfigFile::default();
            let (status, _via) = provider_status(&provider, &config);
            assert_eq!(status, "not configured");
        });
    }

    #[test]
    fn provider_auth_type_non_primary_first_key_is_used_when_no_primary() {
        // When no key is marked primary, falls back to first key
        let provider = make_provider(
            "no-primary",
            vec![
                make_key_with_primary("NO_PRIMARY_TOKEN", true, true, false),
                make_key_with_primary("NO_PRIMARY_KEY", true, false, false),
            ],
        );
        // first key is oauth, no primary set, so first key is used
        assert_eq!(provider_auth_type(&provider), "oauth");
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
            opengoose_secrets::ProviderMeta {
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
        let config = opengoose_secrets::ConfigFile::default();
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
            let config = opengoose_secrets::ConfigFile::default();
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
            let config = opengoose_secrets::ConfigFile::default();
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
            let config = opengoose_secrets::ConfigFile::default();
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
}
