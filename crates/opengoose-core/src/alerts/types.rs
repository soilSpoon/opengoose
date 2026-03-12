//! Core types and error definitions for the alert subsystem.

use std::time::Duration;

pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(300); // 5 minutes

/// Error type for alert dispatch failures.
#[derive(Debug, thiserror::Error)]
pub enum AlertDispatchError {
    #[error("persistence error: {0}")]
    Persistence(#[from] opengoose_persistence::PersistenceError),
    #[error("webhook request failed: {0}")]
    Webhook(#[from] reqwest::Error),
}
