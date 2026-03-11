use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use opengoose_persistence::{AlertStore, Database, ScheduleStore, SessionStore};
use opengoose_types::{
    ChannelMetricsSnapshot, ChannelMetricsStore, ComponentHealth, HealthComponents, HealthResponse,
    HealthStatus,
};

use crate::data::{MetricCard, StatusComponentView, StatusGatewayView, StatusPageView};

const KNOWN_GATEWAY_PLATFORMS: &[&str] = &["discord", "slack", "telegram", "matrix"];

#[derive(Clone, Debug)]
struct ComponentProbe {
    status: HealthStatus,
    detail: String,
    error_detail: Option<String>,
    last_check: String,
}

#[derive(Clone, Debug)]
struct GatewayProbe {
    component: ComponentProbe,
    uptime_label: String,
    tracked: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GatewayCounts {
    healthy: usize,
    degraded: usize,
    unavailable: usize,
}

#[derive(Clone, Debug)]
struct HealthProbe {
    version: &'static str,
    checked_at: String,
    overall_status: HealthStatus,
    database: ComponentProbe,
    cron_scheduler: ComponentProbe,
    alert_dispatcher: ComponentProbe,
    gateways: BTreeMap<String, GatewayProbe>,
    gateway_counts: GatewayCounts,
}

trait HealthStatusViewExt {
    fn label(self) -> &'static str;
    fn tone(self) -> &'static str;
    fn rank(self) -> u8;
}

impl GatewayCounts {
    fn total(self) -> usize {
        self.healthy + self.degraded + self.unavailable
    }

    fn record(&mut self, status: HealthStatus) {
        match status {
            HealthStatus::Healthy => self.healthy += 1,
            HealthStatus::Degraded => self.degraded += 1,
            HealthStatus::Unavailable => self.unavailable += 1,
        }
    }
}

impl HealthStatusViewExt for HealthStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Healthy => "Healthy",
            Self::Degraded => "Degraded",
            Self::Unavailable => "Unavailable",
        }
    }

    fn tone(self) -> &'static str {
        match self {
            Self::Healthy => "success",
            Self::Degraded => "amber",
            Self::Unavailable => "rose",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Healthy => 0,
            Self::Degraded => 1,
            Self::Unavailable => 2,
        }
    }
}

pub fn probe_health(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
) -> Result<HealthResponse> {
    Ok(health_response(&build_health_probe(db, channel_metrics)))
}

pub fn probe_readiness(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
) -> Result<(HealthResponse, bool)> {
    let probe = build_health_probe(db, channel_metrics);
    let ready = is_ready(&probe);
    Ok((health_response(&probe), ready))
}

pub fn load_status_page(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
) -> Result<StatusPageView> {
    Ok(status_page(&build_health_probe(db, channel_metrics)))
}

fn build_health_probe(db: Arc<Database>, channel_metrics: ChannelMetricsStore) -> HealthProbe {
    let checked_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let database = database_probe(db.clone(), &checked_at);
    let cron_scheduler = schedule_probe(db.clone(), &checked_at);
    let alert_dispatcher = alert_probe(db.clone(), &checked_at);
    let gateways = build_gateway_probes(channel_metrics.snapshot(), &checked_at);
    let gateway_counts = gateway_counts(&gateways);

    let critical_status = worst_status([
        database.status,
        cron_scheduler.status,
        alert_dispatcher.status,
    ]);
    let tracked_gateway_status = worst_status(
        gateways
            .values()
            .filter(|gateway| gateway.tracked)
            .map(|gateway| gateway.component.status),
    );
    let overall_status = worse_status(critical_status, tracked_gateway_status);

    HealthProbe {
        version: env!("CARGO_PKG_VERSION"),
        checked_at,
        overall_status,
        database,
        cron_scheduler,
        alert_dispatcher,
        gateways,
        gateway_counts,
    }
}

fn database_probe(db: Arc<Database>, checked_at: &str) -> ComponentProbe {
    match SessionStore::new(db).stats() {
        Ok(stats) => component_probe(
            HealthStatus::Healthy,
            checked_at,
            format!(
                "SQLite reachable with {} session(s) and {} stored message(s).",
                stats.session_count, stats.message_count
            ),
            None,
        ),
        Err(error) => component_probe(
            HealthStatus::Unavailable,
            checked_at,
            "Session store query failed.".into(),
            Some(error.to_string()),
        ),
    }
}

fn schedule_probe(db: Arc<Database>, checked_at: &str) -> ComponentProbe {
    match ScheduleStore::new(db).list() {
        Ok(schedules) => component_probe(
            HealthStatus::Healthy,
            checked_at,
            format!(
                "Cron scheduler storage responded with {} schedule(s).",
                schedules.len()
            ),
            None,
        ),
        Err(error) => component_probe(
            HealthStatus::Unavailable,
            checked_at,
            "Cron scheduler storage query failed.".into(),
            Some(error.to_string()),
        ),
    }
}

fn alert_probe(db: Arc<Database>, checked_at: &str) -> ComponentProbe {
    let store = AlertStore::new(db);
    match (store.list(), store.current_metrics()) {
        (Ok(rules), Ok(_metrics)) => component_probe(
            HealthStatus::Healthy,
            checked_at,
            format!(
                "Alert dispatcher storage responded with {} rule(s).",
                rules.len()
            ),
            None,
        ),
        (Err(error), _) | (_, Err(error)) => component_probe(
            HealthStatus::Unavailable,
            checked_at,
            "Alert dispatcher queries failed.".into(),
            Some(error.to_string()),
        ),
    }
}

fn build_gateway_probes(
    snapshots: std::collections::HashMap<String, ChannelMetricsSnapshot>,
    checked_at: &str,
) -> BTreeMap<String, GatewayProbe> {
    let mut platforms = KNOWN_GATEWAY_PLATFORMS
        .iter()
        .map(|platform| (*platform).to_string())
        .collect::<BTreeSet<_>>();
    platforms.extend(snapshots.keys().cloned());

    platforms
        .into_iter()
        .map(|platform| {
            let probe = match snapshots.get(&platform) {
                Some(snapshot) => tracked_gateway_probe(snapshot, checked_at),
                None => idle_gateway_probe(checked_at),
            };
            (platform, probe)
        })
        .collect()
}

fn tracked_gateway_probe(snapshot: &ChannelMetricsSnapshot, checked_at: &str) -> GatewayProbe {
    if let Some(uptime_secs) = snapshot.uptime_secs {
        GatewayProbe {
            component: component_probe(
                HealthStatus::Healthy,
                checked_at,
                format!("Gateway connected for {}.", format_uptime(uptime_secs)),
                None,
            ),
            uptime_label: format_uptime(uptime_secs),
            tracked: true,
        }
    } else {
        GatewayProbe {
            component: component_probe(
                HealthStatus::Degraded,
                checked_at,
                format!(
                    "Gateway is reconnecting after {} attempt(s).",
                    snapshot.reconnect_count
                ),
                snapshot.last_error.clone(),
            ),
            uptime_label: "Reconnecting".into(),
            tracked: true,
        }
    }
}

fn idle_gateway_probe(checked_at: &str) -> GatewayProbe {
    GatewayProbe {
        component: component_probe(
            HealthStatus::Unavailable,
            checked_at,
            "No connection telemetry has been reported yet.".into(),
            None,
        ),
        uptime_label: "Awaiting connection".into(),
        tracked: false,
    }
}

fn component_probe(
    status: HealthStatus,
    checked_at: &str,
    detail: String,
    error_detail: Option<String>,
) -> ComponentProbe {
    ComponentProbe {
        status,
        detail,
        error_detail,
        last_check: checked_at.to_string(),
    }
}

fn gateway_counts(gateways: &BTreeMap<String, GatewayProbe>) -> GatewayCounts {
    let mut counts = GatewayCounts::default();
    for gateway in gateways.values() {
        counts.record(gateway.component.status);
    }
    counts
}

fn is_ready(probe: &HealthProbe) -> bool {
    [
        probe.database.status,
        probe.cron_scheduler.status,
        probe.alert_dispatcher.status,
    ]
    .into_iter()
    .all(|status| status == HealthStatus::Healthy)
}

fn worst_status(statuses: impl IntoIterator<Item = HealthStatus>) -> HealthStatus {
    statuses
        .into_iter()
        .max_by_key(|status| status.rank())
        .unwrap_or(HealthStatus::Healthy)
}

fn worse_status(left: HealthStatus, right: HealthStatus) -> HealthStatus {
    if left.rank() >= right.rank() {
        left
    } else {
        right
    }
}

fn format_uptime(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn overall_summary(probe: &HealthProbe) -> String {
    if probe.database.status == HealthStatus::Unavailable
        || probe.cron_scheduler.status == HealthStatus::Unavailable
        || probe.alert_dispatcher.status == HealthStatus::Unavailable
    {
        return "One or more core services are unavailable. Check the database, scheduler, and alerting stores first.".into();
    }

    if probe.gateway_counts.degraded > 0 {
        return format!(
            "Core services are healthy, but {} gateway connection(s) are reconnecting.",
            probe.gateway_counts.degraded
        );
    }

    if probe
        .gateways
        .values()
        .all(|gateway| !gateway.tracked && gateway.component.status == HealthStatus::Unavailable)
    {
        return "Core services are healthy. Gateway telemetry will appear once adapters report their first connection state.".into();
    }

    "Database, scheduler, alerting, and tracked gateways are healthy.".into()
}

fn gateway_summary(counts: GatewayCounts) -> String {
    format!(
        "{} healthy / {} degraded / {} unavailable",
        counts.healthy, counts.degraded, counts.unavailable
    )
}

fn component_health(probe: &ComponentProbe) -> ComponentHealth {
    ComponentHealth {
        status: probe.status,
        last_check: probe.last_check.clone(),
        error_detail: probe.error_detail.clone(),
    }
}

fn health_response(probe: &HealthProbe) -> HealthResponse {
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

fn status_page(probe: &HealthProbe) -> StatusPageView {
    StatusPageView {
        overall_label: probe.overall_status.label().into(),
        overall_tone: probe.overall_status.tone(),
        snapshot_label: format!("Snapshot {}", probe.checked_at),
        summary: overall_summary(probe),
        metrics: vec![
            MetricCard {
                label: "Overall".into(),
                value: probe.overall_status.label().into(),
                note: format!("OpenGoose {}", probe.version),
                tone: probe.overall_status.tone(),
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
        ],
        components: vec![
            StatusComponentView {
                name: "Database".into(),
                status_label: probe.database.status.label().into(),
                status_tone: probe.database.status.tone(),
                detail: probe.database.detail.clone(),
            },
            StatusComponentView {
                name: "Cron scheduler".into(),
                status_label: probe.cron_scheduler.status.label().into(),
                status_tone: probe.cron_scheduler.status.tone(),
                detail: probe.cron_scheduler.detail.clone(),
            },
            StatusComponentView {
                name: "Alert dispatcher".into(),
                status_label: probe.alert_dispatcher.status.label().into(),
                status_tone: probe.alert_dispatcher.status.tone(),
                detail: probe.alert_dispatcher.detail.clone(),
            },
        ],
        gateways: probe
            .gateways
            .iter()
            .map(|(platform, gateway)| StatusGatewayView {
                platform: platform.clone(),
                status_label: gateway.component.status.label().into(),
                status_tone: gateway.component.status.tone(),
                uptime_label: gateway.uptime_label.clone(),
                detail: gateway.component.detail.clone(),
            })
            .collect(),
        gateway_summary: gateway_summary(probe.gateway_counts),
        gateway_empty_hint:
            "Gateway adapters will appear here once they report connection telemetry.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use opengoose_types::{Platform, SessionKey};

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().expect("db should open"))
    }

    #[test]
    fn health_probe_reports_healthy_when_gateways_are_quiet() {
        let response = probe_health(test_db(), ChannelMetricsStore::new())
            .expect("health probe should succeed");

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

        assert_eq!(page.overall_label, "Healthy");
        assert_eq!(
            page.summary,
            "Database, scheduler, alerting, and tracked gateways are healthy."
        );
        assert_eq!(
            page.gateway_summary,
            "1 healthy / 0 degraded / 3 unavailable"
        );
        assert_eq!(page.gateways.len(), 4);
    }
}
