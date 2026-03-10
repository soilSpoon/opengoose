pub mod agents;
pub mod dashboard;
pub mod runs;
pub mod sessions;
pub mod teams;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Convert a persistence/profile/team error into an HTTP 500 response.
pub(crate) struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("internal error: {}", self.0),
        )
            .into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
