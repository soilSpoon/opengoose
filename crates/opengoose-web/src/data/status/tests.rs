use std::collections::BTreeMap;
use std::sync::Arc;

use opengoose_persistence::{Database, SessionStore};
use opengoose_types::{
    ChannelMetricsSnapshot, ChannelMetricsStore, HealthStatus, Platform, SessionKey,
};

use super::model::{GatewayCounts, GatewayProbe, HealthProbe};
use super::probe::{build_health_probe, component_probe, tracked_gateway_probe};
use super::summary::format_uptime;
use super::view_model::health_response;
use super::{load_status_page, probe_health, probe_readiness};

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().expect("db should open"))
}

#[test]
fn health_probe_reports_healthy_when_gateways_are_quiet() {
    let response =
        probe_health(test_db(), ChannelMetricsStore::new()).expect("health probe should succeed");

    assert_eq!(response.status, HealthStatus::Healthy);
    assert_eq!(
        response.components.gateways["discord"].status,
        HealthStatus::Unavailable
    );
    assert_eq!(
        response.components.cron_scheduler.status,
        HealthStatus::Healthy
    );
}

#[test]
fn readiness_only_depends_on_critical_components() {
    let (response, ready) =
        probe_readiness(test_db(), ChannelMetricsStore::new()).expect("probe should succeed");

    assert!(ready);
    assert_eq!(response.status, HealthStatus::Healthy);
}

#[test]
fn health_probe_marks_retrying_gateways_as_degraded() {
    let metrics = ChannelMetricsStore::new();
    metrics.record_reconnect("slack", Some("timeout".into()));

    let response = probe_health(test_db(), metrics).expect("health probe should succeed");

    assert_eq!(response.status, HealthStatus::Degraded);
    assert_eq!(
        response.components.gateways["slack"].status,
        HealthStatus::Degraded
    );
    assert_eq!(
        response.components.gateways["slack"]
            .error_detail
            .as_deref(),
        Some("timeout")
    );
}

#[test]
fn format_uptime_formats_seconds_only() {
    assert_eq!(format_uptime(42), "42s");
}

#[test]
fn format_uptime_formats_minutes_and_seconds() {
    assert_eq!(format_uptime(125), "2m 5s");
}

#[test]
fn format_uptime_formats_hours_and_minutes() {
    assert_eq!(format_uptime(3665), "1h 1m");
}

#[test]
fn tracked_gateway_probe_uses_last_error_details_when_present() {
    let probe = tracked_gateway_probe(
        &ChannelMetricsSnapshot {
            uptime_secs: None,
            reconnect_count: 3,
            last_error: Some("timeout".into()),
        },
        "2026-03-11T00:00:00Z",
    );

    assert_eq!(probe.component.status, HealthStatus::Degraded);
    assert_eq!(
        probe.component.detail,
        "Gateway is reconnecting after 3 attempt(s)."
    );
    assert_eq!(probe.component.error_detail.as_deref(), Some("timeout"));
}

#[test]
fn tracked_gateway_probe_uses_uptime_for_connected_gateways() {
    let probe = tracked_gateway_probe(
        &ChannelMetricsSnapshot {
            uptime_secs: Some(65),
            reconnect_count: 2,
            last_error: None,
        },
        "2026-03-11T00:00:00Z",
    );

    assert_eq!(probe.component.status, HealthStatus::Healthy);
    assert_eq!(probe.component.detail, "Gateway connected for 1m 5s.");
    assert_eq!(probe.uptime_label, "1m 5s");
}

#[test]
fn build_health_probe_sorts_known_gateways_and_reports_database_counts() {
    let db = test_db();
    let metrics = ChannelMetricsStore::new();
    let key = SessionKey::new(Platform::Discord, "studio-a", "ops");
    let sessions = SessionStore::new(db.clone());

    sessions
        .append_user_message(&key, "Need a health check", Some("pm"))
        .unwrap();
    metrics.set_connected("telegram");
    metrics.set_connected("discord");

    let probe = build_health_probe(db, metrics);

    assert_eq!(probe.overall_status, HealthStatus::Healthy);
    assert_eq!(probe.gateway_counts.healthy, 2);
    assert_eq!(probe.gateway_counts.unavailable, 2);
    assert_eq!(
        probe.gateways.keys().cloned().collect::<Vec<_>>(),
        vec!["discord", "matrix", "slack", "telegram"]
    );
    assert!(probe.database.detail.contains("1 session(s)"));
    assert!(probe.database.detail.contains("1 stored message(s)"));
}

#[test]
fn build_health_probe_marks_mixed_gateway_states_as_degraded() {
    let metrics = ChannelMetricsStore::new();
    metrics.set_connected("discord");
    metrics.record_reconnect("slack", Some("timeout".into()));

    let probe = build_health_probe(test_db(), metrics);

    assert_eq!(probe.overall_status, HealthStatus::Degraded);
    assert_eq!(probe.gateway_counts.healthy, 1);
    assert_eq!(probe.gateway_counts.degraded, 1);
}

#[test]
fn health_response_conversion_maps_component_and_gateway_fields() {
    let probe = HealthProbe {
        version: "1.2.3",
        checked_at: "2026-03-10T12:00:00Z".into(),
        overall_status: HealthStatus::Degraded,
        database: component_probe(
            HealthStatus::Healthy,
            "2026-03-10T12:00:00Z",
            "db ok".into(),
            None,
        ),
        cron_scheduler: component_probe(
            HealthStatus::Healthy,
            "2026-03-10T12:00:00Z",
            "scheduler ok".into(),
            None,
        ),
        alert_dispatcher: component_probe(
            HealthStatus::Healthy,
            "2026-03-10T12:00:00Z",
            "alerts ok".into(),
            None,
        ),
        gateways: BTreeMap::from([(
            "slack".into(),
            GatewayProbe {
                component: component_probe(
                    HealthStatus::Degraded,
                    "2026-03-10T12:00:00Z",
                    "Gateway is reconnecting after 3 attempt(s).".into(),
                    Some("timeout".into()),
                ),
                uptime_label: "Reconnecting".into(),
                tracked: true,
            },
        )]),
        gateway_counts: GatewayCounts {
            healthy: 0,
            degraded: 1,
            unavailable: 0,
        },
    };

    let response = health_response(&probe);

    assert_eq!(response.status, HealthStatus::Degraded);
    assert_eq!(response.version, "1.2.3");
    assert_eq!(response.components.database.status, HealthStatus::Healthy);
    assert_eq!(
        response.components.gateways["slack"].status,
        HealthStatus::Degraded
    );
    assert_eq!(
        response.components.gateways["slack"]
            .error_detail
            .as_deref(),
        Some("timeout")
    );
}

#[test]
fn status_page_conversion_uses_gateway_summary() {
    let metrics = ChannelMetricsStore::new();
    metrics.set_connected("discord");

    let page = load_status_page(test_db(), metrics).expect("page should build");

    assert_eq!(page.intro.mode_label, "Healthy");
    assert_eq!(
        page.intro.summary,
        "Database, scheduler, alerting, and tracked gateways are healthy."
    );
    assert_eq!(
        page.gateway_panel.subtitle,
        "1 healthy / 0 degraded / 3 unavailable"
    );
    assert_eq!(page.gateway_panel.cards.len(), 4);
}
