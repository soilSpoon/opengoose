use super::shared::{MetaRow, Notice};

/// A clickable status filter shown above the plugins catalog.
#[derive(Clone)]
pub struct PluginFilterItem {
    pub label: String,
    pub count: usize,
    pub tone: &'static str,
    pub page_url: String,
    pub active: bool,
}

/// Summary row for the plugin list sidebar.
#[derive(Clone)]
pub struct PluginListItem {
    pub title: String,
    pub subtitle: String,
    pub preview: String,
    pub status_detail: String,
    pub search_text: String,
    pub source_label: String,
    pub source_badge: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub page_url: String,
    pub active: bool,
}

/// Full detail/action panel for a selected plugin or install placeholder.
#[derive(Clone)]
pub struct PluginDetailView {
    pub title: String,
    pub subtitle: String,
    pub source_label: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub lifecycle_label: String,
    pub lifecycle_tone: &'static str,
    pub runtime_label: String,
    pub runtime_tone: &'static str,
    pub status_summary: String,
    pub runtime_note: Option<String>,
    pub meta: Vec<MetaRow>,
    pub capabilities: Vec<String>,
    pub capabilities_hint: String,
    pub registered_skills: Vec<String>,
    pub missing_skills: Vec<String>,
    pub notice: Option<Notice>,
    pub install_source_path: String,
    pub toggle_label: String,
    pub delete_label: String,
    pub is_placeholder: bool,
}

/// View-model for the plugins page (list + selected detail).
#[derive(Clone)]
pub struct PluginsPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub filters: Vec<PluginFilterItem>,
    pub plugins: Vec<PluginListItem>,
    pub selected: PluginDetailView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_detail_view_placeholder_defaults() {
        let detail = PluginDetailView {
            title: "No plugins installed".into(),
            subtitle: "Install a plugin directory to populate the catalog.".into(),
            source_label: "Plugin registry".into(),
            status_label: "No plugins".into(),
            status_tone: "neutral",
            lifecycle_label: "Awaiting install".into(),
            lifecycle_tone: "neutral",
            runtime_label: "No runtime data".into(),
            runtime_tone: "neutral",
            status_summary: "Install a plugin to inspect runtime readiness.".into(),
            runtime_note: None,
            meta: vec![],
            capabilities: vec![],
            capabilities_hint: "Installed plugin capabilities will appear here.".into(),
            registered_skills: vec![],
            missing_skills: vec![],
            notice: None,
            install_source_path: String::new(),
            toggle_label: "Enable plugin".into(),
            delete_label: String::new(),
            is_placeholder: true,
        };

        assert!(detail.is_placeholder);
        assert!(detail.capabilities_hint.contains("capabilities"));
        assert!(detail.toggle_label.contains("Enable"));
    }
}
