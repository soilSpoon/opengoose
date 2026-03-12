mod model;
mod probe;
mod summary;
#[cfg(test)]
mod tests;
mod view_model;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;
use opengoose_types::{ChannelMetricsStore, HealthResponse};

use self::probe::build_health_probe;
use self::summary::is_ready;
use self::view_model::{health_response, status_page};
use crate::data::views::StatusPageView;

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
