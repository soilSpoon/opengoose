use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Typed error for web API and page handlers.
///
/// Implements `IntoResponse` so Axum handlers can return
/// `Result<T, WebError>` and get proper HTTP status codes
/// with a consistent JSON error body for API routes.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// Resource not found (HTTP 404).
    #[error("not found: {0}")]
    NotFound(String),

    /// Client sent an invalid request (HTTP 400).
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Request body is well-formed but semantically invalid (HTTP 422).
    #[error("unprocessable entity: {0}")]
    UnprocessableEntity(String),

    /// Unexpected server-side failure (HTTP 500).
    #[error("internal error: {0}")]
    Internal(String),

    /// Propagated from the persistence layer.
    #[error("persistence error: {0}")]
    Persistence(#[from] opengoose_persistence::PersistenceError),

    /// Propagated from team store operations.
    #[error("team error: {0}")]
    Team(#[from] opengoose_teams::TeamError),

    /// Propagated from profile store operations.
    #[error("profile error: {0}")]
    Profile(#[from] opengoose_profiles::ProfileError),

    /// Template rendering failure.
    #[error("template error: {0}")]
    Template(#[from] askama::Error),

    /// Unauthorized request (HTTP 401).
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// Catch-all for other errors.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// JSON body returned for API errors.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl WebError {
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Persistence(err) => err.is_transient(),
            Self::Team(err) => err.is_transient(),
            Self::Profile(err) => err.is_transient(),
            Self::Other(err) => anyhow_error_is_transient(err),
            _ => false,
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::UnprocessableEntity(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Persistence(e) if e.is_not_found() => StatusCode::NOT_FOUND,
            Self::Team(opengoose_teams::TeamError::NotFound(_)) => StatusCode::NOT_FOUND,
            Self::Team(opengoose_teams::TeamError::AlreadyExists(_)) => StatusCode::CONFLICT,
            Self::Team(opengoose_teams::TeamError::Store(
                opengoose_types::YamlStoreError::NotFound { .. },
            )) => StatusCode::NOT_FOUND,
            Self::Team(opengoose_teams::TeamError::Store(
                opengoose_types::YamlStoreError::AlreadyExists { .. },
            )) => StatusCode::CONFLICT,
            Self::Team(opengoose_teams::TeamError::Store(
                opengoose_types::YamlStoreError::ValidationFailed(_),
            )) => StatusCode::BAD_REQUEST,
            Self::Profile(opengoose_profiles::ProfileError::NotFound(_)) => StatusCode::NOT_FOUND,
            Self::Profile(opengoose_profiles::ProfileError::AlreadyExists(_)) => {
                StatusCode::CONFLICT
            }
            Self::Profile(opengoose_profiles::ProfileError::Store(
                opengoose_types::YamlStoreError::NotFound { .. },
            )) => StatusCode::NOT_FOUND,
            Self::Profile(opengoose_profiles::ProfileError::Store(
                opengoose_types::YamlStoreError::AlreadyExists { .. },
            )) => StatusCode::CONFLICT,
            Self::Profile(opengoose_profiles::ProfileError::Store(
                opengoose_types::YamlStoreError::ValidationFailed(_),
            )) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Template(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

fn anyhow_error_is_transient(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(opengoose_types::is_transient_io_error)
            || cause
                .downcast_ref::<opengoose_persistence::PersistenceError>()
                .is_some_and(opengoose_persistence::PersistenceError::is_transient)
            || cause
                .downcast_ref::<opengoose_teams::TeamError>()
                .is_some_and(opengoose_teams::TeamError::is_transient)
            || cause
                .downcast_ref::<opengoose_profiles::ProfileError>()
                .is_some_and(opengoose_profiles::ProfileError::is_transient)
    })
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorBody {
            error: self.to_string(),
        };
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_returns_404() {
        let err = WebError::NotFound("page missing".into());
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn bad_request_returns_400() {
        let err = WebError::BadRequest("invalid input".into());
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn unprocessable_entity_returns_422() {
        let err = WebError::UnprocessableEntity("field out of range".into());
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn internal_returns_500() {
        let err = WebError::Internal("unexpected".into());
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn team_not_found_returns_404() {
        let err = WebError::Team(opengoose_teams::TeamError::NotFound("t1".into()));
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn team_already_exists_returns_409() {
        let err = WebError::Team(opengoose_teams::TeamError::AlreadyExists("t1".into()));
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn team_validation_returns_400() {
        let err = WebError::Team(opengoose_teams::TeamError::Store(
            opengoose_types::YamlStoreError::ValidationFailed("bad".into()),
        ));
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn profile_not_found_returns_404() {
        let err = WebError::Profile(opengoose_profiles::ProfileError::NotFound("p1".into()));
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn template_error_returns_500() {
        let err = WebError::Template(askama::Error::Fmt);
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn profile_already_exists_returns_409() {
        let err = WebError::Profile(opengoose_profiles::ProfileError::AlreadyExists("p1".into()));
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn profile_validation_returns_400() {
        let err = WebError::Profile(opengoose_profiles::ProfileError::Store(
            opengoose_types::YamlStoreError::ValidationFailed("bad".into()),
        ));
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn other_error_returns_500() {
        let err = WebError::Other(anyhow::anyhow!("unexpected"));
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn persistence_not_found_returns_404() {
        let err = WebError::Persistence(opengoose_persistence::PersistenceError::Database(
            diesel::result::Error::NotFound,
        ));
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn internal_error_display_message() {
        let err = WebError::Internal("db failed".into());
        assert_eq!(err.to_string(), "internal error: db failed");
    }

    #[test]
    fn not_found_display_message() {
        let err = WebError::NotFound("session xyz".into());
        assert_eq!(err.to_string(), "not found: session xyz");
    }

    #[test]
    fn bad_request_display_message() {
        let err = WebError::BadRequest("missing field".into());
        assert_eq!(err.to_string(), "bad request: missing field");
    }

    #[test]
    fn into_response_returns_correct_status() {
        let err = WebError::NotFound("test".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn persistence_timeout_is_transient() {
        let err = WebError::Persistence(opengoose_persistence::PersistenceError::Io(
            std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out"),
        ));
        assert!(err.is_transient());
    }

    #[test]
    fn bad_request_is_not_transient() {
        let err = WebError::BadRequest("bad input".into());
        assert!(!err.is_transient());
    }

    #[test]
    fn anyhow_timeout_is_transient() {
        let err = WebError::Other(anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "timed out",
        )));
        assert!(err.is_transient());
    }
}
