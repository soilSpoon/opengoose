//! Stable sample view-model builders for render tests and benchmarks.

use crate::data::{
    ActivityItem, AlertCard, DashboardView, GatewayCard, GatewayPanelView, HeroLiveIntroView,
    MetricCard, MetricGridView, MonitorBannerView, RunListItem, SessionListItem, StatusSegment,
    TrendBar,
};

pub fn sample_dashboard_view() -> DashboardView {
    DashboardView {
        intro: HeroLiveIntroView {
            id: String::new(),
            eyebrow: "Signal board".into(),
            title: "Track sessions, orchestration, and queue pressure from one surface.".into(),
            summary:
                "The dashboard stays server-rendered for resilience, then refreshes the live board whenever the runtime event stream moves."
                    .into(),
            transport_label: "Live transport".into(),
            mode_tone: "success",
            mode_label: "Live runtime".into(),
            status_summary: "Server-sent events stream fresh snapshots.".into(),
            status_id: String::new(),
            status_note:
                "Live snapshots re-render the board below as session, run, and queue events arrive."
                    .into(),
        },
        banner: MonitorBannerView {
            eyebrow: "Live snapshot".into(),
            title: "Runs, queue pressure, and agent chatter stay in one place.".into(),
            summary: "Server-sent events stream fresh snapshots.".into(),
            mode_tone: "success",
            mode_label: "Live runtime".into(),
            stream_label: "Event stream + fallback sweep".into(),
            snapshot_label: "Snapshot 12:34:56 UTC".into(),
        },
        metric_grid: MetricGridView {
            class_name: "metric-grid".into(),
            items: vec![MetricCard {
                label: "Active runs".into(),
                value: "2".into(),
                note: "1 completed in the latest window".into(),
                tone: "amber",
            }],
        },
        queue_cards: vec![MetricCard {
            label: "Pending".into(),
            value: "4".into(),
            note: "Waiting for pickup".into(),
            tone: "cyan",
        }],
        run_segments: vec![StatusSegment {
            label: "Running".into(),
            value: "2".into(),
            tone: "cyan",
            width: 100,
        }],
        queue_segments: vec![StatusSegment {
            label: "Pending".into(),
            value: "4".into(),
            tone: "amber",
            width: 100,
        }],
        duration_bars: vec![TrendBar {
            label: "feature-dev".into(),
            value: "7m 12s".into(),
            detail: "running".into(),
            tone: "cyan",
            height: 76,
        }],
        activities: vec![ActivityItem {
            actor: "frontend-engineer".into(),
            meta: "Directed to reviewer".into(),
            detail: "Live dashboard shell refreshed over SSE.".into(),
            timestamp: "2026-03-10 12:34".into(),
            tone: "cyan",
        }],
        alerts: vec![AlertCard {
            eyebrow: "Runtime Active".into(),
            title: "2 orchestration runs currently active".into(),
            description: "The dashboard is streaming run status and queue pressure.".into(),
            tone: "success",
        }],
        sessions: vec![SessionListItem {
            title: "ops / bridge".into(),
            subtitle: "feature-dev active · Live runtime".into(),
            preview: "Review the live dashboard state.".into(),
            updated_at: "2026-03-10 12:34".into(),
            badge: "DISCORD".into(),
            badge_tone: "cyan",
            page_url: "/sessions?session=ops".into(),
            active: false,
        }],
        runs: vec![RunListItem {
            title: "feature-dev".into(),
            subtitle: "chain workflow · Live runtime".into(),
            updated_at: "2026-03-10 12:34".into(),
            progress_label: "2/4 steps".into(),
            badge: "RUNNING".into(),
            badge_tone: "cyan",
            page_url: "/runs?run=run-1".into(),
            queue_page_url: "/queue?run=run-1".into(),
            active: false,
        }],
        gateway_panel: GatewayPanelView {
            title: "Gateway status".into(),
            subtitle: "Connection state for each channel adapter".into(),
            empty_hint: String::new(),
            cards: vec![GatewayCard {
                platform: "Slack".into(),
                state_label: "Connected".into(),
                state_tone: "success",
                uptime_label: "1d".into(),
                detail: String::new(),
            }],
        },
    }
}

pub fn sample_dashboard_view_large(n: usize) -> DashboardView {
    let mut dashboard = sample_dashboard_view();
    for i in 1..n {
        dashboard.sessions.push(SessionListItem {
            title: format!("session-{i}"),
            subtitle: format!("team-{i} active"),
            preview: format!("Message preview {i}"),
            updated_at: "2026-03-10 12:00".into(),
            badge: "DISCORD".into(),
            badge_tone: "cyan",
            page_url: format!("/sessions?session=s{i}"),
            active: false,
        });
        dashboard.runs.push(RunListItem {
            title: format!("run-{i}"),
            subtitle: "chain workflow".into(),
            updated_at: "2026-03-10 12:00".into(),
            progress_label: format!("{i}/10 steps"),
            badge: "RUNNING".into(),
            badge_tone: "cyan",
            page_url: format!("/runs?run=r{i}"),
            queue_page_url: format!("/queue?run=r{i}"),
            active: false,
        });
        dashboard.activities.push(ActivityItem {
            actor: format!("agent-{i}"),
            meta: "status update".into(),
            detail: format!("Agent {i} completed step {i}"),
            timestamp: "2026-03-10 12:00".into(),
            tone: "cyan",
        });
    }
    dashboard
}
