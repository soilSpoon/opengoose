use opengoose_types::HealthStatus;

use super::model::{GatewayCounts, HealthProbe};

pub(super) trait HealthStatusViewExt {
    fn label(self) -> &'static str;
    fn tone(self) -> &'static str;
    fn rank(self) -> u8;
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

pub(super) fn is_ready(probe: &HealthProbe) -> bool {
    [
        probe.database.status,
        probe.cron_scheduler.status,
        probe.alert_dispatcher.status,
    ]
    .into_iter()
    .all(|status| status == HealthStatus::Healthy)
}

pub(super) fn worst_status(statuses: impl IntoIterator<Item = HealthStatus>) -> HealthStatus {
    statuses
        .into_iter()
        .max_by_key(|status| status.rank())
        .unwrap_or(HealthStatus::Healthy)
}

pub(super) fn worse_status(left: HealthStatus, right: HealthStatus) -> HealthStatus {
    if left.rank() >= right.rank() {
        left
    } else {
        right
    }
}

pub(super) fn format_uptime(seconds: u64) -> String {
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

pub(super) fn overall_summary(probe: &HealthProbe) -> String {
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

pub(super) fn gateway_summary(counts: GatewayCounts) -> String {
    format!(
        "{} healthy / {} degraded / {} unavailable",
        counts.healthy, counts.degraded, counts.unavailable
    )
}
