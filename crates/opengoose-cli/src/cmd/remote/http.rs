use crate::error::{CliError, CliResult};
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct RemoteAgentInfo {
    pub(super) name: String,
    pub(super) capabilities: Vec<String>,
    pub(super) endpoint: String,
    pub(super) connected_secs: u64,
    pub(super) last_heartbeat_secs: u64,
}

pub(super) async fn cmd_list(base_url: &str) -> CliResult<()> {
    let url = list_url(base_url);
    let resp = reqwest::get(&url).await.map_err(|e| {
        CliError::Validation(format!(
            "failed to connect to web server at {base_url}: {e}\nIs `opengoose web` running?"
        ))
    })?;

    if !resp.status().is_success() {
        return Err(CliError::Validation(format!(
            "server returned {} when listing remote agents",
            resp.status()
        )));
    }

    let agents: Vec<RemoteAgentInfo> = resp.json().await?;

    if agents.is_empty() {
        println!("No remote agents connected.");
        return Ok(());
    }

    println!(
        "{:<20} {:<24} {:<12} {:<12} CAPABILITIES",
        "NAME", "ENDPOINT", "CONNECTED", "HEARTBEAT"
    );
    for agent in &agents {
        println!(
            "{:<20} {:<24} {:<12} {:<12} {}",
            agent.name,
            agent.endpoint,
            format_duration(agent.connected_secs),
            format_duration(agent.last_heartbeat_secs),
            agent.capabilities.join(", "),
        );
    }

    println!("\n{} remote agent(s) connected.", agents.len());
    Ok(())
}

pub(super) fn list_url(base_url: &str) -> String {
    format!("{}/api/agents/remote", base_url.trim_end_matches('/'))
}

pub(super) async fn cmd_disconnect(name: &str, base_url: &str) -> CliResult<()> {
    let url = disconnect_url(base_url, name);
    let client = reqwest::Client::new();
    let resp = client.delete(&url).send().await.map_err(|e| {
        CliError::Validation(format!(
            "failed to connect to web server at {base_url}: {e}\nIs `opengoose web` running?"
        ))
    })?;

    if resp.status().is_success() {
        println!("Disconnected remote agent '{name}'.");
    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CliError::Validation(format!("remote agent '{name}' is not connected")));
    } else {
        return Err(CliError::Validation(format!("server returned {}", resp.status())));
    }

    Ok(())
}

pub(super) fn disconnect_url(base_url: &str, name: &str) -> String {
    format!(
        "{}/api/agents/remote/{}",
        base_url.trim_end_matches('/'),
        urlencoding::encode(name)
    )
}

pub(super) fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3600)
    }
}
