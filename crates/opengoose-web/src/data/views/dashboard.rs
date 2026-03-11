use super::runs::RunListItem;
use super::sessions::SessionListItem;
use super::shared::{ActivityItem, AlertCard, GatewayCard, MetricCard, StatusSegment, TrendBar};

/// Aggregated view-model for the main dashboard page.
#[allow(dead_code)]
#[derive(Clone)]
pub struct DashboardView {
    pub mode_label: String,
    pub mode_tone: &'static str,
    pub stream_summary: String,
    pub snapshot_label: String,
    pub metrics: Vec<MetricCard>,
    pub queue_cards: Vec<MetricCard>,
    pub run_segments: Vec<StatusSegment>,
    pub queue_segments: Vec<StatusSegment>,
    pub duration_bars: Vec<TrendBar>,
    pub activities: Vec<ActivityItem>,
    pub alerts: Vec<AlertCard>,
    pub sessions: Vec<SessionListItem>,
    pub runs: Vec<RunListItem>,
    pub gateways: Vec<GatewayCard>,
}
