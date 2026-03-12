use std::collections::BTreeMap;

use opengoose_types::HealthStatus;

#[derive(Clone, Debug)]
pub(super) struct ComponentProbe {
    pub(super) status: HealthStatus,
    pub(super) detail: String,
    pub(super) error_detail: Option<String>,
    pub(super) last_check: String,
}

#[derive(Clone, Debug)]
pub(super) struct GatewayProbe {
    pub(super) component: ComponentProbe,
    pub(super) uptime_label: String,
    pub(super) tracked: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct GatewayCounts {
    pub(super) healthy: usize,
    pub(super) degraded: usize,
    pub(super) unavailable: usize,
}

#[derive(Clone, Debug)]
pub(super) struct HealthProbe {
    pub(super) version: &'static str,
    pub(super) checked_at: String,
    pub(super) overall_status: HealthStatus,
    pub(super) database: ComponentProbe,
    pub(super) cron_scheduler: ComponentProbe,
    pub(super) alert_dispatcher: ComponentProbe,
    pub(super) gateways: BTreeMap<String, GatewayProbe>,
    pub(super) gateway_counts: GatewayCounts,
}

impl GatewayCounts {
    pub(super) fn total(self) -> usize {
        self.healthy + self.degraded + self.unavailable
    }

    pub(super) fn record(&mut self, status: HealthStatus) {
        match status {
            HealthStatus::Healthy => self.healthy += 1,
            HealthStatus::Degraded => self.degraded += 1,
            HealthStatus::Unavailable => self.unavailable += 1,
        }
    }
}
