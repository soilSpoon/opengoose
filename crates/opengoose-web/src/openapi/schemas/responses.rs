use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "NotFound": {
            "description": "Resource not found",
            "content": {
                "application/json": {
                    "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                }
            }
        },
        "InternalError": {
            "description": "Internal server error",
            "content": {
                "application/json": {
                    "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                }
            }
        },
        "UnprocessableEntity": {
            "description": "Unprocessable entity — validation failed",
            "content": {
                "application/json": {
                    "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                }
            }
        }
    })
}
