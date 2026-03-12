use super::{
    ActivityItem, AlertCard, CalloutCardView, GatewayPanelView, HeroLiveIntroView, MetricCard,
    MetricGridView, MonitorBannerView, RunListItem, SessionListItem, StatusSegment, TrendBar,
};

/// Aggregated view-model for the main dashboard page.
#[derive(Clone)]
pub struct DashboardView {
    pub intro: HeroLiveIntroView,
    pub banner: MonitorBannerView,
    pub metric_grid: MetricGridView,
    pub queue_cards: Vec<MetricCard>,
    pub run_segments: Vec<StatusSegment>,
    pub queue_segments: Vec<StatusSegment>,
    pub duration_bars: Vec<TrendBar>,
    pub activities: Vec<ActivityItem>,
    pub alerts: Vec<AlertCard>,
    pub sessions: Vec<SessionListItem>,
    pub runs: Vec<RunListItem>,
    pub gateway_panel: GatewayPanelView,
}

/// View-model for the dedicated system status page.
#[derive(Clone)]
pub struct StatusPageView {
    pub intro: HeroLiveIntroView,
    pub banner: MonitorBannerView,
    pub metric_grid: MetricGridView,
    pub component_cards: Vec<CalloutCardView>,
    pub gateway_panel: GatewayPanelView,
}
