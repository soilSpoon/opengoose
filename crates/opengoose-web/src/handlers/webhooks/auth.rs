use axum::body::Bytes;
use axum::http::HeaderMap;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::warn;

use super::AppError;
use super::payload::MatchedWebhookTrigger;

pub(super) const DEFAULT_SIGNATURE_HEADER: &str = "X-Webhook-Signature";
pub(super) const DEFAULT_TIMESTAMP_HEADER: &str = "X-Webhook-Timestamp";
const DEFAULT_TIMESTAMP_TOLERANCE_SECS: i64 = 300;

type HmacSha256 = Hmac<Sha256>;

pub(super) fn validate_request(
    matching: &[MatchedWebhookTrigger],
    headers: &HeaderMap,
    body: &Bytes,
    normalized_path: &str,
) -> Result<(), AppError> {
    let provided_secret = header_value(headers, "X-Webhook-Secret");

    for matched in matching {
        let trigger_name = matched.trigger.name.as_str();

        validate_plaintext_secret(
            &matched.condition,
            provided_secret,
            normalized_path,
            trigger_name,
        )?;
        validate_hmac_signature(
            &matched.condition,
            headers,
            body.as_ref(),
            normalized_path,
            trigger_name,
        )?;
    }

    Ok(())
}

fn validate_plaintext_secret(
    condition: &opengoose_teams::triggers::WebhookCondition,
    provided_secret: Option<&str>,
    normalized_path: &str,
    trigger_name: &str,
) -> Result<(), AppError> {
    let Some(expected) = condition.secret.as_deref() else {
        return Ok(());
    };

    match provided_secret {
        Some(secret) if secret == expected => Ok(()),
        _ => {
            warn!(
                path = %normalized_path,
                trigger = %trigger_name,
                "webhook secret invalid or missing"
            );
            Err(AppError::Unauthorized(
                "invalid or missing webhook secret".into(),
            ))
        }
    }
}

fn validate_hmac_signature(
    condition: &opengoose_teams::triggers::WebhookCondition,
    headers: &HeaderMap,
    body: &[u8],
    normalized_path: &str,
    trigger_name: &str,
) -> Result<(), AppError> {
    let Some(secret) = condition.hmac_secret.as_deref() else {
        return Ok(());
    };

    let signature_header = condition
        .signature_header
        .as_deref()
        .unwrap_or(DEFAULT_SIGNATURE_HEADER);
    let timestamp_header = condition
        .timestamp_header
        .as_deref()
        .unwrap_or(DEFAULT_TIMESTAMP_HEADER);
    let tolerance_secs = condition
        .timestamp_tolerance_secs
        .unwrap_or(DEFAULT_TIMESTAMP_TOLERANCE_SECS)
        .max(0);

    let timestamp = header_value(headers, timestamp_header).ok_or_else(|| {
        unauthorized_signature(normalized_path, trigger_name, "missing webhook timestamp")
    })?;
    let timestamp_epoch = timestamp.parse::<i64>().map_err(|_| {
        unauthorized_signature(normalized_path, trigger_name, "invalid webhook timestamp")
    })?;
    let age_secs = (Utc::now().timestamp() - timestamp_epoch).abs();
    if age_secs > tolerance_secs {
        return Err(unauthorized_signature(
            normalized_path,
            trigger_name,
            "webhook timestamp outside replay window",
        ));
    }

    let provided_signature = header_value(headers, signature_header).ok_or_else(|| {
        unauthorized_signature(normalized_path, trigger_name, "missing webhook signature")
    })?;
    let provided_signature = provided_signature
        .strip_prefix("sha256=")
        .unwrap_or(provided_signature);
    let provided_bytes = hex::decode(provided_signature).map_err(|_| {
        unauthorized_signature(normalized_path, trigger_name, "invalid webhook signature")
    })?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|error| AppError::Internal(format!("invalid webhook signing key: {error}")))?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);

    mac.verify_slice(&provided_bytes).map_err(|_| {
        unauthorized_signature(normalized_path, trigger_name, "invalid webhook signature")
    })
}

fn header_value<'a>(headers: &'a HeaderMap, header_name: &str) -> Option<&'a str> {
    headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
}

fn unauthorized_signature(normalized_path: &str, trigger_name: &str, message: &str) -> AppError {
    warn!(
        path = %normalized_path,
        trigger = %trigger_name,
        reason = %message,
        "webhook signature validation failed"
    );
    AppError::Unauthorized(message.into())
}
