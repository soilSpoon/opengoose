use std::io::Write;

use anyhow::{Result, bail};
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_provider_bridge::{ConfigKeySummary, GooseProviderService, ProviderSummary};
use opengoose_secrets::ConfigFile;

pub(super) async fn login(provider_arg: Option<&str>, output: CliOutput) -> Result<()> {
    let providers = GooseProviderService::list_providers().await;

    let provider = match provider_arg {
        Some(id) => match providers.iter().find(|p| p.name == id) {
            Some(provider) => provider,
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

    let has_oauth = provider.config_keys.iter().any(|key| key.oauth_flow);

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

    for key in provider.config_keys.iter().filter(|key| key.oauth_flow) {
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
    for key in provider.config_keys.iter().filter(|key| !key.oauth_flow) {
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
        for (index, model) in models.iter().take(10).enumerate() {
            println!("  {:>2}. {}", index + 1, model);
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

pub(super) async fn list(output: CliOutput) -> Result<()> {
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

pub(super) async fn models(provider_name: &str, output: CliOutput) -> Result<()> {
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

fn prompt_provider_selection(providers: &[ProviderSummary]) -> Result<&ProviderSummary> {
    let items: Vec<_> = providers
        .iter()
        .filter(|provider| !provider.config_keys.is_empty())
        .collect();

    eprintln!("Select a provider:");
    for (index, provider) in items.iter().enumerate() {
        let auth_hint = if provider.config_keys.iter().any(|key| key.oauth_flow) {
            " (OAuth)"
        } else {
            ""
        };
        eprintln!("  [{:>2}] {}{auth_hint}", index + 1, provider.display_name);
    }
    eprintln!();
    eprint!("Enter number: ");
    std::io::stderr().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    let index = input
        .trim()
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("invalid selection"))?;

    items
        .get(index.wrapping_sub(1))
        .copied()
        .ok_or_else(|| anyhow::anyhow!("selection out of range (enter 1–{})", items.len()))
}

pub(super) fn key_label(key: &ConfigKeySummary) -> &str {
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
        Some(default) => format!("  {label} [{} (default: {default})]: ", key.name),
        None => format!("  {label} [{}]: ", key.name),
    };
    eprint!("{prompt}");
    std::io::stderr().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    if trimmed.is_empty()
        && let Some(default) = &key.default
    {
        return Ok(default.clone());
    }
    Ok(trimmed)
}

pub(super) fn provider_auth_type(provider: &ProviderSummary) -> &'static str {
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

pub(super) fn provider_status(
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
