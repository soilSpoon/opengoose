use axum::Json;
use axum::extract::State;
use serde::Serialize;

use opengoose_types::ChannelMetricsSnapshot;

use super::AppError;
use crate::state::AppState;

/// JSON response for the live per-platform channel connection metrics.
#[derive(Serialize)]
pub struct ChannelMetricsResponse {
    pub platforms: std::collections::HashMap<String, ChannelMetricsSnapshot>,
}

/// GET /api/channel-metrics — return the in-memory channel adapter metrics snapshot.
pub async fn get_channel_metrics(
    State(state): State<AppState>,
) -> Result<Json<ChannelMetricsResponse>, AppError> {
    Ok(Json(ChannelMetricsResponse {
        platforms: state.channel_metrics.snapshot(),
    }))
}
