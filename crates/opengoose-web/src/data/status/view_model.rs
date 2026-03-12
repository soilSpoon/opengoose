use opengoose_types::{ComponentHealth, HealthComponents, HealthResponse};

use super::model::{ComponentProbe, HealthProbe};
use super::summary::{HealthStatusViewExt, gateway_summary, overall_summary, worst_status};
use crate::data::views::{
    CalloutCardView, GatewayCard, GatewayPanelView, HeroLiveIntroView, MetricCard, MetricGridView,
    MonitorBannerView, StatusPageView,
};

fn component_health(probe: &ComponentProbe) -> ComponentHealth {
    ComponentHealth {
        status: probe.status,
        last_check: probe.last_check.clone(),
        error_detail: probe.error_detail.clone(),
    }
}

pub(super) fn health_response(probe: &HealthProbe) -> HealthResponse {
    HealthResponse {
        status: probe.overall_status,
        version: probe.version.to_string(),
        checked_at: probe.checked_at.clone(),
        components: HealthComponents {
            database: component_health(&probe.database),
            cron_scheduler: component_health(&probe.cron_scheduler),
            alert_dispatcher: component_health(&probe.alert_dispatcher),
            gateways: probe
                .gateways
                .iter()
                .map(|(platform, gateway)| (platform.clone(), component_health(&gateway.component)))
                .collect(),
        },
    }
}

pub(super) fn status_page(probe: &HealthProbe) -> StatusPageView {
    let overall_label = probe.overall_status.label().to_string();
    let overall_tone = probe.overall_status.tone();
    let snapshot_label = format!("Snapshot {}", probe.checked_at);
    let summary = overall_summary(probe);
    let metrics = vec![
        MetricCard {
            label: "Overall".into(),
            value: overall_label.clone(),
            note: format!("OpenGoose {}", probe.version),
            tone: overall_tone,
        },
        MetricCard {
            label: "Database".into(),
            value: probe.database.status.label().into(),
            note: probe.database.detail.clone(),
            tone: probe.database.status.tone(),
        },
        MetricCard {
            label: "Scheduler".into(),
            value: probe.cron_scheduler.status.label().into(),
            note: probe.cron_scheduler.detail.clone(),
            tone: probe.cron_scheduler.status.tone(),
        },
        MetricCard {
            label: "Alerts".into(),
            value: probe.alert_dispatcher.status.label().into(),
            note: probe.alert_dispatcher.detail.clone(),
            tone: probe.alert_dispatcher.status.tone(),
        },
        MetricCard {
            label: "Gateways".into(),
            value: probe.gateway_counts.total().to_string(),
            note: gateway_summary(probe.gateway_counts),
            tone: worst_status(
                probe
                    .gateways
                    .values()
                    .map(|gateway| gateway.component.status),
            )
            .tone(),
        },
    ];

    StatusPageView {
        intro: HeroLiveIntroView {
            id: "status-page-intro".into(),
            eyebrow: "System status".into(),
            title: "Watch core runtime health without leaving the dashboard.".into(),
            summary: summary.clone(),
            transport_label: "Live transport".into(),
            mode_tone: overall_tone,
            mode_label: overall_label.clone(),
            status_summary: snapshot_label.clone(),
            status_id: String::new(),
            status_note: "The health board patches on runtime events and falls back to a slower reconciliation sweep if the stream stays quiet.".into(),
        },
        banner: MonitorBannerView {
            eyebrow: "Health snapshot".into(),
            title: "Database, scheduler, alerts, and gateway telemetry in one view.".into(),
            summary: summary.clone(),
            mode_tone: overall_tone,
            mode_label: overall_label.clone(),
            stream_label: "Event stream + fallback sweep".into(),
            snapshot_label: snapshot_label.clone(),
        },
        metric_grid: MetricGridView {
            class_name: "metric-grid".into(),
            items: metrics.clone(),
        },
        component_cards: vec![
            CalloutCardView {
                eyebrow: "Database".into(),
                title: probe.database.status.label().into(),
                description: probe.database.detail.clone(),
                tone: probe.database.status.tone(),
            },
            CalloutCardView {
                eyebrow: "Cron scheduler".into(),
                title: probe.cron_scheduler.status.label().into(),
                description: probe.cron_scheduler.detail.clone(),
                tone: probe.cron_scheduler.status.tone(),
            },
            CalloutCardView {
                eyebrow: "Alert dispatcher".into(),
                title: probe.alert_dispatcher.status.label().into(),
                description: probe.alert_dispatcher.detail.clone(),
                tone: probe.alert_dispatcher.status.tone(),
            },
        ],
        gateway_panel: GatewayPanelView {
            title: "Gateway connections".into(),
            subtitle: gateway_summary(probe.gateway_counts),
            empty_hint: "Gateway adapters will appear here once they report connection telemetry."
                .into(),
            cards: probe
                .gateways
                .iter()
                .map(|(platform, gateway)| GatewayCard {
                    platform: platform.clone(),
                    state_label: gateway.component.status.label().into(),
                    state_tone: gateway.component.status.tone(),
                    uptime_label: gateway.uptime_label.clone(),
                    detail: gateway.component.detail.clone(),
                })
                .collect(),
        },
    }
}
