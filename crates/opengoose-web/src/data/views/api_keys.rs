use super::{MetricCard, Notice};

/// A single API key row on the dashboard security table.
#[derive(Clone)]
pub struct ApiKeyRowView {
    pub id: String,
    pub description_label: String,
    pub created_at: String,
    pub last_used_at: String,
    pub usage_label: String,
    pub usage_tone: &'static str,
}

/// One-time plaintext reveal returned immediately after generation.
#[derive(Clone)]
pub struct GeneratedApiKeyView {
    pub id: String,
    pub plaintext: String,
    pub description_label: String,
}

/// View-model for the API keys dashboard page.
#[derive(Clone)]
pub struct ApiKeysPageView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub summary: String,
    pub metrics: Vec<MetricCard>,
    pub keys: Vec<ApiKeyRowView>,
    pub notice: Option<Notice>,
    pub generated_key: Option<GeneratedApiKeyView>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_api_key_view_keeps_plaintext_and_description() {
        let view = GeneratedApiKeyView {
            id: "key-1".into(),
            plaintext: "ogk_secret".into(),
            description_label: "Remote agent".into(),
        };

        assert!(view.plaintext.starts_with("ogk_"));
        assert_eq!(view.description_label, "Remote agent");
    }
}
