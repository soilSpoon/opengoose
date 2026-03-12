use serde::Deserialize;

/// Matrix error response body returned on non-2xx status codes.
///
/// See the [Matrix spec error codes](https://spec.matrix.org/v1.6/client-server-api/#standard-error-response).
#[derive(Deserialize, Debug)]
pub struct MatrixError {
    /// Machine-readable error code (e.g. `M_FORBIDDEN`, `M_UNKNOWN`).
    pub errcode: Option<String>,
    /// Human-readable error message.
    pub error: Option<String>,
}
