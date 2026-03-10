pub mod agents;
pub mod dashboard;
pub mod runs;
pub mod sessions;
pub mod teams;

/// Handler-level error type.
///
/// This is a type alias for [`crate::error::WebError`], which provides
/// domain-specific HTTP status code mapping and consistent JSON error bodies.
pub type AppError = crate::error::WebError;
