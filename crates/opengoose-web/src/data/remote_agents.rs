use anyhow::Result;
use chrono::Utc;
use opengoose_teams::remote::RemoteAgentRegistry;
use urlencoding::encode;

use crate::data::views::{MetricCard, RemoteAgentRowView, RemoteAgentsPageView};

/// Load the remote agents page view-model from the shared registry.
pub async fn load_remote_agents_page(
    registry: &RemoteAgentRegistry,
    websocket_url: String,
) -> Result<RemoteAgentsPageView> {
    let mut connected = registry.list().await;
    connected.sort_by(|left, right| left.name.cmp(&right.name));

    let interval_secs = registry.heartbeat_interval().as_secs();
    let timeout_secs = registry.heartbeat_timeout().as_secs();
    let mut healthy_count = 0usize;
    let mut late_count = 0usize;
    let mut stale_count = 0usize;

    let agents = connected
        .into_iter()
        .map(|agent| {
            let connected_secs = agent.connected_at.elapsed().as_secs();
            let heartbeat_secs = agent.last_heartbeat.elapsed().as_secs();
            let (status_label, status_tone) = if heartbeat_secs > timeout_secs {
                stale_count += 1;
                ("Stale", "danger")
            } else if heartbeat_secs > interval_secs {
                late_count += 1;
                ("Late", "amber")
            } else {
                healthy_count += 1;
                ("Healthy", "success")
            };
            let capabilities_text = if agent.capabilities.is_empty() {
                "No capabilities advertised".into()
            } else {
                agent.capabilities.join(", ")
            };

            RemoteAgentRowView {
                name: agent.name.clone(),
                capabilities: agent.capabilities,
                capabilities_text,
                endpoint: agent.endpoint,
                connected_for: format_elapsed(connected_secs),
                connected_sort: connected_secs.to_string(),
                heartbeat_age: format_elapsed(heartbeat_secs),
                heartbeat_sort: heartbeat_secs.to_string(),
                status_label: status_label.into(),
                status_tone,
                disconnect_path: format!("/api/agents/remote/{}", encode(&agent.name)),
            }
        })
        .collect();

    let total = healthy_count + late_count + stale_count;
    let (mode_label, mode_tone) = if total == 0 {
        ("Idle registry".into(), "neutral")
    } else if stale_count > 0 {
        ("Attention needed".into(), "danger")
    } else if late_count > 0 {
        ("Heartbeat drift".into(), "amber")
    } else {
        ("Live registry".into(), "success")
    };

    Ok(RemoteAgentsPageView {
        mode_label,
        mode_tone,
        stream_summary:
            "The registry snapshot is server-rendered, then refreshed every four seconds through the existing SSE transport."
                .into(),
        snapshot_label: format!("Snapshot {}", Utc::now().format("%H:%M:%S UTC")),
        metrics: vec![
            MetricCard {
                label: "Connected".into(),
                value: total.to_string(),
                note: "Currently registered remote agents".into(),
                tone: "cyan",
            },
            MetricCard {
                label: "Healthy".into(),
                value: healthy_count.to_string(),
                note: format!("Heartbeat within {}", format_elapsed(interval_secs)),
                tone: "sage",
            },
            MetricCard {
                label: "Late".into(),
                value: late_count.to_string(),
                note: "Past the nominal heartbeat interval".into(),
                tone: "amber",
            },
            MetricCard {
                label: "Stale".into(),
                value: stale_count.to_string(),
                note: format!("Past the {} timeout window", format_elapsed(timeout_secs)),
                tone: "rose",
            },
        ],
        agents,
        websocket_url,
        heartbeat_interval_label: format_elapsed(interval_secs),
        heartbeat_timeout_label: format_elapsed(timeout_secs),
        handshake_preview: serde_json::to_string_pretty(&serde_json::json!({
            "type": "handshake",
            "agent_name": "remote-builder-1",
            "api_key": "your-shared-key",
            "capabilities": ["execute", "relay"]
        }))?,
    })
}

fn format_elapsed(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_teams::remote::RemoteConfig;

    #[tokio::test]
    async fn load_remote_agents_page_marks_empty_registry_idle() {
        let page = load_remote_agents_page(
            &RemoteAgentRegistry::new(RemoteConfig::default()),
            "ws://localhost:3000/api/agents/connect".into(),
        )
        .await
        .expect("page should load");

        assert_eq!(page.mode_label, "Idle registry");
        assert!(page.agents.is_empty());
    }
}
