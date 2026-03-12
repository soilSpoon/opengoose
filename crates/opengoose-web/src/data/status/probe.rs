use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use chrono::{SecondsFormat, Utc};
use opengoose_persistence::{AlertStore, Database, ScheduleStore, SessionStore};
use opengoose_types::{ChannelMetricsSnapshot, ChannelMetricsStore, HealthStatus};

use super::model::{ComponentProbe, GatewayCounts, GatewayProbe, HealthProbe};
use super::summary::{format_uptime, worse_status, worst_status};

const KNOWN_GATEWAY_PLATFORMS: &[&str] = &["discord", "slack", "telegram", "matrix"];

pub(super) fn build_health_probe(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
) -> HealthProbe {
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
    snapshots: HashMap<String, ChannelMetricsSnapshot>,
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

pub(super) fn tracked_gateway_probe(
    snapshot: &ChannelMetricsSnapshot,
    checked_at: &str,
) -> GatewayProbe {
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

pub(super) fn component_probe(
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
