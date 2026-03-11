use super::shared::MetricCard;

/// A single component status card on the system status page.
#[derive(Clone)]
pub struct StatusComponentView {
    pub name: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub detail: String,
}

/// A single gateway health row on the system status page.
#[derive(Clone)]
pub struct StatusGatewayView {
    pub platform: String,
    pub status_label: String,
    pub status_tone: &'static str,
    pub uptime_label: String,
    pub detail: String,
}

/// View-model for the dedicated system status page.
#[derive(Clone)]
pub struct StatusPageView {
    pub overall_label: String,
    pub overall_tone: &'static str,
    pub snapshot_label: String,
    pub summary: String,
    pub metrics: Vec<MetricCard>,
    pub components: Vec<StatusComponentView>,
    pub gateways: Vec<StatusGatewayView>,
    pub gateway_summary: String,
    pub gateway_empty_hint: String,
}
