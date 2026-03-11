use std::io::Write;

use anyhow::{Result, anyhow};

use opengoose_provider_bridge::{ConfigKeySummary, ProviderSummary};

pub(super) fn prompt_provider_selection(providers: &[ProviderSummary]) -> Result<&ProviderSummary> {
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
        .map_err(|_| anyhow!("invalid selection"))?;

    items
        .get(idx.wrapping_sub(1))
        .copied()
        .ok_or_else(|| anyhow!("selection out of range (enter 1–{})", items.len()))
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

pub(super) fn prompt_text_input(key: &ConfigKeySummary) -> Result<String> {
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
