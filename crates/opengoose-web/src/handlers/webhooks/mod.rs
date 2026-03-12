mod auth;
mod dispatch;
mod payload;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};

use super::AppError;
use crate::state::AppState;

pub use payload::WebhookResponse;

/// POST /api/webhooks/*path — receive an inbound webhook and fire matching triggers.
///
/// Looks up all enabled `webhook_received` triggers and checks whether any
/// match the incoming path (prefix match). If a trigger has a `secret`
/// configured in its condition, the caller must supply it in the
/// `X-Webhook-Secret` request header. If a trigger has an `hmac_secret`,
/// the caller must also provide a valid HMAC-SHA256 signature over
/// `timestamp.body`, plus a timestamp inside the allowed replay window.
pub async fn receive_webhook(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), AppError> {
    let normalized_path = payload::normalize_path(&path);
    let matching = payload::find_matching_triggers(&state, &normalized_path)?;

    auth::validate_request(&matching, &headers, &body, &normalized_path)?;

    Ok((
        StatusCode::OK,
        Json(payload::accepted_response(
            dispatch::dispatch_matching_triggers(&state, &normalized_path, matching),
        )),
    ))
}

#[cfg(test)]
mod tests;
