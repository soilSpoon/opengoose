use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use opengoose_persistence::{Database, SessionStore};
use opengoose_types::{ChannelMetricsSnapshot, ChannelMetricsStore, EventBus};
use serde::Serialize;

use crate::data::{MetricCard, StatusComponentView, StatusGatewayView, StatusPageView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeStatus {
    Ok,
    Degraded,
}

impl ProbeStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Degraded => "degraded",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Degraded => "Degraded",
        }
    }

    fn tone(self) -> &'static str {
        match self {
            Self::Ok => "success",
            Self::Degraded => "rose",
        }
    }
}

#[derive(Clone, Debug)]
struct ComponentProbe {
    status: ProbeStatus,
    detail: String,
}

#[derive(Clone, Debug)]
struct GatewayProbe {
    platform: String,
    status: ProbeStatus,
    uptime_secs: Option<u64>,
    reconnect_count: u32,
    last_error: Option<String>,
    detail: String,
}

#[derive(Clone, Debug)]
struct HealthProbe {
    version: &'static str,
    checked_at: String,
    overall_status: ProbeStatus,
    database: ComponentProbe,
    event_bus: ComponentProbe,
    gateway_connections: ComponentProbe,
    gateways: Vec<GatewayProbe>,
    connected_gateways: usize,
    reconnecting_gateways: usize,
}

#[derive(Clone, Serialize)]
pub struct HealthComponentResponse {
    pub status: &'static str,
    pub detail: String,
}

#[derive(Clone, Serialize)]
pub struct GatewayHealthResponse {
    pub platform: String,
    pub status: &'static str,
    pub uptime_secs: Option<u64>,
    pub reconnect_count: u32,
    pub last_error: Option<String>,
    pub detail: String,
}

#[derive(Clone, Serialize)]
pub struct HealthComponentsResponse {
    pub database: HealthComponentResponse,
    pub event_bus: HealthComponentResponse,
    pub gateway_connections: HealthComponentResponse,
}

#[derive(Clone, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub checked_at: String,
    pub components: HealthComponentsResponse,
    pub gateways: Vec<GatewayHealthResponse>,
}

pub fn probe_health(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
    event_bus: EventBus,
) -> Result<HealthResponse> {
    let probe = build_health_probe(db, channel_metrics, event_bus)?;
    Ok(HealthResponse::from(&probe))
}

pub fn load_status_page(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
    event_bus: EventBus,
) -> Result<StatusPageView> {
    let probe = build_health_probe(db, channel_metrics, event_bus)?;
    Ok(StatusPageView::from(&probe))
}

fn build_health_probe(
    db: Arc<Database>,
    channel_metrics: ChannelMetricsStore,
    event_bus: EventBus,
) -> Result<HealthProbe> {
    let session_stats = SessionStore::new(db).stats()?;
    let database = ComponentProbe {
        status: ProbeStatus::Ok,
        detail: format!(
            "SQLite reachable with {} session(s) and {} stored message(s).",
            session_stats.session_count, session_stats.message_count
        ),
    };

    let _receiver = event_bus.subscribe();
    let event_bus = ComponentProbe {
        status: ProbeStatus::Ok,
        detail: "Broadcast channel is available for live runtime updates.".into(),
    };

    let mut gateways = channel_metrics
        .snapshot()
        .into_iter()
        .map(|(platform, snapshot)| gateway_probe(platform, snapshot))
        .collect::<Vec<_>>();
    gateways.sort_by(|left, right| left.platform.cmp(&right.platform));

    let connected_gateways = gateways
        .iter()
        .filter(|gateway| gateway.status == ProbeStatus::Ok)
        .count();
    let reconnecting_gateways = gateways.len().saturating_sub(connected_gateways);
    let gateway_connections = if gateways.is_empty() {
        ComponentProbe {
            status: ProbeStatus::Ok,
            detail: "No gateway metrics have been reported yet.".into(),
        }
    } else if reconnecting_gateways > 0 {
        ComponentProbe {
            status: ProbeStatus::Degraded,
            detail: format!(
                "{connected_gateways} connected and {reconnecting_gateways} reconnecting."
            ),
        }
    } else {
        ComponentProbe {
            status: ProbeStatus::Ok,
            detail: format!("{connected_gateways} gateway connection(s) healthy."),
        }
    };

    let overall_status = if matches!(gateway_connections.status, ProbeStatus::Degraded) {
        ProbeStatus::Degraded
    } else {
        ProbeStatus::Ok
    };

    Ok(HealthProbe {
        version: env!("CARGO_PKG_VERSION"),
        checked_at: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        overall_status,
        database,
        event_bus,
        gateway_connections,
        gateways,
        connected_gateways,
        reconnecting_gateways,
    })
}

fn gateway_probe(platform: String, snapshot: ChannelMetricsSnapshot) -> GatewayProbe {
    let status = if snapshot.uptime_secs.is_some() {
        ProbeStatus::Ok
    } else {
        ProbeStatus::Degraded
    };
    let detail = if let Some(error) = snapshot.last_error.clone() {
        format!(
            "{} reconnect attempt(s); last error: {}",
            snapshot.reconnect_count, error
        )
    } else if let Some(uptime_secs) = snapshot.uptime_secs {
        format!("Connected for {}.", format_uptime(uptime_secs))
    } else {
        format!(
            "{} reconnect attempt(s) recorded.",
            snapshot.reconnect_count
        )
    };

    GatewayProbe {
        platform,
        status,
        uptime_secs: snapshot.uptime_secs,
        reconnect_count: snapshot.reconnect_count,
        last_error: snapshot.last_error,
        detail,
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

impl From<&HealthProbe> for HealthResponse {
    fn from(probe: &HealthProbe) -> Self {
        Self {
            status: probe.overall_status.as_str(),
            version: probe.version,
            checked_at: probe.checked_at.clone(),
            components: HealthComponentsResponse {
                database: HealthComponentResponse {
                    status: probe.database.status.as_str(),
                    detail: probe.database.detail.clone(),
                },
                event_bus: HealthComponentResponse {
                    status: probe.event_bus.status.as_str(),
                    detail: probe.event_bus.detail.clone(),
                },
                gateway_connections: HealthComponentResponse {
                    status: probe.gateway_connections.status.as_str(),
                    detail: probe.gateway_connections.detail.clone(),
                },
            },
            gateways: probe
                .gateways
                .iter()
                .map(|gateway| GatewayHealthResponse {
                    platform: gateway.platform.clone(),
                    status: gateway.status.as_str(),
                    uptime_secs: gateway.uptime_secs,
                    reconnect_count: gateway.reconnect_count,
                    last_error: gateway.last_error.clone(),
                    detail: gateway.detail.clone(),
                })
                .collect(),
        }
    }
}

impl From<&HealthProbe> for StatusPageView {
    fn from(probe: &HealthProbe) -> Self {
        Self {
            overall_label: probe.overall_status.label().into(),
            overall_tone: probe.overall_status.tone(),
            snapshot_label: format!("Snapshot {}", probe.checked_at),
            summary: if probe.overall_status == ProbeStatus::Ok {
                "Database access, the event bus, and tracked gateway telemetry are all responding."
                    .into()
            } else {
                "One or more tracked gateway connections are retrying. Core services still respond."
                    .into()
            },
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
                    label: "Event bus".into(),
                    value: probe.event_bus.status.label().into(),
                    note: probe.event_bus.detail.clone(),
                    tone: probe.event_bus.status.tone(),
                },
                MetricCard {
                    label: "Gateways".into(),
                    value: probe.gateways.len().to_string(),
                    note: format!(
                        "{} connected / {} reconnecting",
                        probe.connected_gateways, probe.reconnecting_gateways
                    ),
                    tone: probe.gateway_connections.status.tone(),
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
                    name: "Event bus".into(),
                    status_label: probe.event_bus.status.label().into(),
                    status_tone: probe.event_bus.status.tone(),
                    detail: probe.event_bus.detail.clone(),
                },
                StatusComponentView {
                    name: "Gateway connections".into(),
                    status_label: probe.gateway_connections.status.label().into(),
                    status_tone: probe.gateway_connections.status.tone(),
                    detail: probe.gateway_connections.detail.clone(),
                },
            ],
            gateways: probe
                .gateways
                .iter()
                .map(|gateway| StatusGatewayView {
                    platform: gateway.platform.clone(),
                    status_label: gateway.status.label().into(),
                    status_tone: gateway.status.tone(),
                    uptime_label: gateway
                        .uptime_secs
                        .map(format_uptime)
                        .unwrap_or_else(|| "Awaiting connection".into()),
                    detail: gateway.detail.clone(),
                })
                .collect(),
            gateway_summary: probe.gateway_connections.detail.clone(),
            gateway_empty_hint:
                "Gateway adapters will appear here once they report connection telemetry.".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_types::ChannelMetricsStore;

    #[test]
    fn health_probe_reports_ok_when_gateways_are_quiet() {
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        let response = probe_health(db, ChannelMetricsStore::new(), EventBus::new(16))
            .expect("health probe should succeed");

        assert_eq!(response.status, "ok");
        assert_eq!(response.components.gateway_connections.status, "ok");
        assert!(response.gateways.is_empty());
    }

    #[test]
    fn health_probe_marks_retrying_gateways_as_degraded() {
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        let metrics = ChannelMetricsStore::new();
        metrics.record_reconnect("slack", Some("timeout".into()));

        let response =
            probe_health(db, metrics, EventBus::new(16)).expect("health probe should succeed");

        assert_eq!(response.status, "degraded");
        assert_eq!(response.components.gateway_connections.status, "degraded");
        assert_eq!(response.gateways[0].platform, "slack");
        assert_eq!(response.gateways[0].status, "degraded");
    }
}
