/// JSON API handlers for agent profiles.
pub mod agents;
/// JSON API handlers for monitoring alert rules and history.
pub mod alerts;
/// JSON API handler for channel connection metrics.
pub mod channel_metrics;
/// JSON API handler for aggregate dashboard statistics.
pub mod dashboard;
/// SSE API handler for live dashboard and sessions updates.
pub mod events;
/// WebSocket gateway and REST endpoints for remote agent connections.
pub mod remote_agents;
/// JSON API handlers for orchestration runs.
pub mod runs;
/// JSON API handlers for chat sessions and messages.
pub mod sessions;
/// JSON API handlers for team definitions.
pub mod teams;
/// JSON API handlers for trigger CRUD and toggle management.
pub mod triggers;
/// HTTP endpoint for receiving inbound webhooks and firing matching triggers.
pub mod webhooks;
/// JSON API handlers for workflow definitions and manual triggers.
pub mod workflows;

#[cfg(test)]
pub(crate) mod test_support;

/// Handler-level error type.
///
/// This is a type alias for [`crate::error::WebError`], which provides
/// domain-specific HTTP status code mapping and consistent JSON error bodies.
pub type AppError = crate::error::WebError;
