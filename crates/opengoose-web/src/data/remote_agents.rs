use anyhow::Result;
use chrono::Utc;
use opengoose_teams::remote::RemoteAgentRegistry;
use urlencoding::encode;

use crate::data::views::{
    CodePanelView, HeroLiveIntroView, MetaPanelView, MetaRow, MetricCard, MetricGridView,
    MonitorBannerView, RemoteAgentRowView, RemoteAgentsPageView,
};

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
                disconnect_path: format!("/remote-agents/{}/disconnect", encode(&agent.name)),
            }
        })
        .collect();

    let total = healthy_count + late_count + stale_count;
    let (mode_label, mode_tone): (String, &'static str) = if total == 0 {
        ("Idle registry".into(), "neutral")
    } else if stale_count > 0 {
        ("Attention needed".into(), "danger")
    } else if late_count > 0 {
        ("Heartbeat drift".into(), "amber")
    } else {
        ("Live registry".into(), "success")
    };

    let stream_summary =
        "The registry snapshot is server-rendered first, then patched whenever the remote-agent registry changes.".to_string();
    let snapshot_label = format!("Snapshot {}", Utc::now().format("%H:%M:%S UTC"));
    let metrics = vec![
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
    ];

    Ok(RemoteAgentsPageView {
        intro: HeroLiveIntroView {
            id: "remote-agents-page-intro".into(),
            eyebrow: "Remote agents".into(),
            title: "Monitor connected remote workers and cut stale sockets from the dashboard.".into(),
            summary: stream_summary.clone(),
            transport_label: "Registry stream".into(),
            mode_tone,
            mode_label: mode_label.clone(),
            status_summary:
                "Heartbeat freshness and connection state patch below whenever the registry changes."
                    .into(),
            status_id: "remote-agents-action-status".into(),
            status_note: "Disconnect actions update this board without a full reload.".into(),
        },
        banner: MonitorBannerView {
            eyebrow: "Remote registry".into(),
            title: "Connected agents, heartbeat drift, and disconnect controls in one live snapshot.".into(),
            summary: stream_summary.clone(),
            mode_tone,
            mode_label: mode_label.clone(),
            stream_label: "Registry events".into(),
            snapshot_label: snapshot_label.clone(),
        },
        metric_grid: MetricGridView {
            class_name: "metric-grid compact-grid".into(),
            items: metrics.clone(),
        },
        agents,
        connection_panel: MetaPanelView {
            title: "Connection endpoint".into(),
            subtitle: "Share this WebSocket URL with any external agent process.".into(),
            rows: vec![
                MetaRow {
                    label: "WebSocket URL".into(),
                    value: websocket_url,
                },
                MetaRow {
                    label: "Handshake timing".into(),
                    value: "Send as the first frame immediately after connect.".into(),
                },
                MetaRow {
                    label: "Server heartbeat".into(),
                    value: format!("Every {}", format_elapsed(interval_secs)),
                },
                MetaRow {
                    label: "Stale timeout".into(),
                    value: format_elapsed(timeout_secs),
                },
            ],
        },
        handshake_panel: CodePanelView {
            title: "Handshake payload".into(),
            subtitle:
                "The first message must identify the agent and include its shared API key."
                    .into(),
            code: serde_json::to_string_pretty(&serde_json::json!({
                "type": "handshake",
                "agent_name": "remote-builder-1",
                "api_key": "your-shared-key",
                "capabilities": ["execute", "relay"]
            }))?,
        },
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

        assert_eq!(page.intro.mode_label, "Idle registry");
        assert!(page.agents.is_empty());
    }
}
