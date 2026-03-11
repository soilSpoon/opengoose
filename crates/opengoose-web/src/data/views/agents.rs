use super::MetricCard;

/// A configuration setting row in the agent detail panel.
#[derive(Clone)]
pub struct SettingRow {
    pub label: String,
    pub value: String,
}

/// An agent extension (skill entry) row in the agent detail panel.
#[derive(Clone)]
pub struct ExtensionRow {
    pub name: String,
    pub kind: String,
    pub summary: String,
}

/// Summary row for the agent list sidebar.
#[derive(Clone)]
pub struct AgentListItem {
    pub title: String,
    pub subtitle: String,
    pub capability: String,
    pub source_label: String,
    pub source_badge: String,
    pub page_url: String,
    pub active: bool,
}

/// Full detail panel for a selected agent profile.
#[derive(Clone)]
pub struct AgentDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub instructions_preview: String,
    pub settings: Vec<SettingRow>,
    pub activities: Vec<String>,
    pub skills: Vec<String>,
    pub extensions: Vec<ExtensionRow>,
    pub yaml: String,
}

/// View-model for the agents page (list + selected detail).
#[derive(Clone)]
pub struct AgentsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub agents: Vec<AgentListItem>,
    pub selected: AgentDetailView,
}

/// A single connected remote agent row in the dashboard table.
#[derive(Clone)]
pub struct RemoteAgentRowView {
    pub name: String,
    pub capabilities: Vec<String>,
    pub capabilities_text: String,
    pub endpoint: String,
    pub connected_for: String,
    pub connected_sort: String,
    pub heartbeat_age: String,
    pub heartbeat_sort: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub disconnect_path: String,
}

/// View-model for the remote agents page.
#[derive(Clone)]
pub struct RemoteAgentsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub stream_summary: String,
    pub snapshot_label: String,
    pub metrics: Vec<MetricCard>,
    pub agents: Vec<RemoteAgentRowView>,
    pub websocket_url: String,
    pub heartbeat_interval_label: String,
    pub heartbeat_timeout_label: String,
    pub handshake_preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_list_item_active_flag_and_url() {
        let item = AgentListItem {
            title: "main".into(),
            subtitle: "Default agent".into(),
            capability: "chat".into(),
            source_label: "Bundled default".into(),
            source_badge: "Bundled default".into(),
            page_url: "/agents?agent=main".into(),
            active: true,
        };
        assert!(item.active);
        assert!(item.page_url.contains("main"));
    }

    #[test]
    fn agent_detail_view_extension_rows() {
        let detail = AgentDetailView {
            title: "main".into(),
            subtitle: "Default".into(),
            source_label: "Bundled".into(),
            instructions_preview: "You are a helpful agent.".into(),
            settings: vec![SettingRow {
                label: "model".into(),
                value: "claude-sonnet-4-6".into(),
            }],
            activities: vec!["chat".into()],
            skills: vec!["memory".into()],
            extensions: vec![ExtensionRow {
                name: "memory".into(),
                kind: "builtin".into(),
                summary: "Stores memories".into(),
            }],
            yaml: "version: 1.0.0".into(),
        };
        assert_eq!(detail.extensions.len(), 1);
        assert_eq!(detail.extensions[0].name, "memory");
        assert_eq!(detail.settings.len(), 1);
    }
}
