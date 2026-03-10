use criterion::{Criterion, criterion_group, criterion_main};
use opengoose_web::data::{
    ActivityItem, AlertCard, DashboardView, MetricCard, RunListItem, SessionListItem,
    StatusSegment, TrendBar,
};
use opengoose_web::render_dashboard_live_partial;

fn sample_dashboard_small() -> DashboardView {
    DashboardView {
        mode_label: "Live runtime".into(),
        mode_tone: "success",
        stream_summary: "Server-sent events stream fresh snapshots.".into(),
        snapshot_label: "Snapshot 12:34:56 UTC".into(),
        metrics: vec![MetricCard {
            label: "Active runs".into(),
            value: "2".into(),
            note: "1 completed in the latest window".into(),
            tone: "amber",
        }],
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
    }
}

fn sample_dashboard_large(n: usize) -> DashboardView {
    let mut d = sample_dashboard_small();
    for i in 1..n {
        d.sessions.push(SessionListItem {
            title: format!("session-{i}"),
            subtitle: format!("team-{i} active"),
            preview: format!("Message preview {i}"),
            updated_at: "2026-03-10 12:00".into(),
            badge: "DISCORD".into(),
            badge_tone: "cyan",
            page_url: format!("/sessions?session=s{i}"),
            active: false,
        });
        d.runs.push(RunListItem {
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
        d.activities.push(ActivityItem {
            actor: format!("agent-{i}"),
            meta: "status update".into(),
            detail: format!("Agent {i} completed step {i}"),
            timestamp: "2026-03-10 12:00".into(),
            tone: "cyan",
        });
    }
    d
}

fn bench_render_dashboard_live_small(c: &mut Criterion) {
    let dashboard = sample_dashboard_small();

    c.bench_function("render_dashboard_live_small", |b| {
        b.iter(|| render_dashboard_live_partial(dashboard.clone()).unwrap());
    });
}

fn bench_render_dashboard_live_large(c: &mut Criterion) {
    let dashboard = sample_dashboard_large(50);

    c.bench_function("render_dashboard_live_large_50", |b| {
        b.iter(|| render_dashboard_live_partial(dashboard.clone()).unwrap());
    });
}

criterion_group!(
    benches,
    bench_render_dashboard_live_small,
    bench_render_dashboard_live_large,
);
criterion_main!(benches);
