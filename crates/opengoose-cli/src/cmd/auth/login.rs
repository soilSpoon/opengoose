use std::io::Write;

use anyhow::{Result, bail};
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_provider_bridge::GooseProviderService;

use super::register::{key_label, prompt_provider_selection, prompt_text_input};

pub(super) async fn cmd_login(provider_arg: Option<&str>, output: CliOutput) -> Result<()> {
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
