/// A single metric card rendered on the dashboard (label, value, footnote, tone).
#[derive(Clone)]
pub struct MetricCard {
    pub label: String,
    pub value: String,
    pub note: String,
    pub tone: &'static str,
}

/// Typed input for a metric grid block.
#[derive(Clone)]
pub struct MetricGridView {
    pub class_name: String,
    pub items: Vec<MetricCard>,
}

/// Typed input for a live hero intro block.
#[derive(Clone)]
pub struct HeroLiveIntroView {
    pub id: String,
    pub eyebrow: String,
    pub title: String,
    pub summary: String,
    pub transport_label: String,
    pub mode_tone: &'static str,
    pub mode_label: String,
    pub status_summary: String,
    pub status_id: String,
    pub status_note: String,
}

/// Typed input for a monitoring banner block.
#[derive(Clone)]
pub struct MonitorBannerView {
    pub eyebrow: String,
    pub title: String,
    pub summary: String,
    pub mode_tone: &'static str,
    pub mode_label: String,
    pub stream_label: String,
    pub snapshot_label: String,
}

/// A reusable eyebrow/title/description/tone callout card.
#[derive(Clone)]
pub struct CalloutCardView {
    pub eyebrow: String,
    pub title: String,
    pub description: String,
    pub tone: &'static str,
}

/// A titled panel that renders gateway status cards.
#[derive(Clone)]
pub struct GatewayPanelView {
    pub title: String,
    pub subtitle: String,
    pub empty_hint: String,
    pub cards: Vec<GatewayCard>,
}

/// A titled panel that renders label/value rows.
#[derive(Clone)]
pub struct MetaPanelView {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<MetaRow>,
}

/// A titled panel that renders a code or payload preview.
#[derive(Clone)]
pub struct CodePanelView {
    pub title: String,
    pub subtitle: String,
    pub code: String,
}

/// An alert banner displayed on the dashboard.
#[derive(Clone)]
pub struct AlertCard {
    pub eyebrow: String,
    pub title: String,
    pub description: String,
    pub tone: &'static str,
}

/// One segment of a stacked status bar (e.g. "Running 3" at 40% width).
#[allow(dead_code)]
#[derive(Clone)]
pub struct StatusSegment {
    pub label: String,
    pub value: String,
    pub tone: &'static str,
    pub width: u8,
}

/// A single bar in the duration trend chart.
#[allow(dead_code)]
#[derive(Clone)]
pub struct TrendBar {
    pub label: String,
    pub value: String,
    pub detail: String,
    pub tone: &'static str,
    pub height: u8,
}

/// One row in the activity feed timeline.
#[allow(dead_code)]
#[derive(Clone)]
pub struct ActivityItem {
    pub actor: String,
    pub meta: String,
    pub detail: String,
    pub timestamp: String,
    pub tone: &'static str,
}

/// A label/value metadata row shown in detail panels.
#[derive(Clone)]
pub struct MetaRow {
    pub label: String,
    pub value: String,
}

/// Option row for a `<select>` field.
#[derive(Clone)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

/// A toast-style notice shown after an action (e.g. team save).
#[derive(Clone)]
pub struct Notice {
    pub text: String,
    pub tone: &'static str,
}

/// A gateway connection status card for the dashboard widget.
#[derive(Clone)]
pub struct GatewayCard {
    pub platform: String,
    pub state_label: String,
    pub state_tone: &'static str,
    pub uptime_label: String,
    pub detail: String,
}
