use opengoose_persistence::Trigger;
use opengoose_teams::triggers::{WebhookCondition, matches_webhook_path};
use serde::Serialize;

use super::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct WebhookResponse {
    pub accepted: bool,
    pub trigger: String,
}

#[derive(Clone)]
pub(super) struct MatchedWebhookTrigger {
    pub(super) trigger: Trigger,
    pub(super) condition: WebhookCondition,
}

pub(super) fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

pub(super) fn find_matching_triggers(
    state: &AppState,
    normalized_path: &str,
) -> Result<Vec<MatchedWebhookTrigger>, AppError> {
    let matching: Vec<_> = state
        .trigger_store
        .list_by_type("webhook_received")?
        .into_iter()
        .filter(|trigger| matches_webhook_path(&trigger.condition_json, normalized_path))
        .map(|trigger| MatchedWebhookTrigger {
            condition: serde_json::from_str(&trigger.condition_json).unwrap_or_default(),
            trigger,
        })
        .collect();

    if matching.is_empty() {
        return Err(AppError::NotFound(format!(
            "no webhook trigger configured for path {normalized_path}"
        )));
    }

    Ok(matching)
}

pub(super) fn accepted_response(trigger: String) -> WebhookResponse {
    WebhookResponse {
        accepted: true,
        trigger,
    }
}
