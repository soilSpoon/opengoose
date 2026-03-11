use serde_json::{Value, json};

/// System-level schemas: errors, health, and metrics.
pub(super) fn build() -> Value {
    json!({
        "ErrorResponse": {
            "type": "object",
            "required": ["error"],
            "properties": {
                "error": { "type": "string", "description": "Human-readable error message" }
            }
        },
        "HealthStatus": {
            "type": "string",
            "enum": ["healthy", "degraded", "unavailable"]
        },
        "ComponentHealth": {
            "type": "object",
            "required": ["status", "last_check"],
            "properties": {
                "status": { "$ref": "#/components/schemas/HealthStatus" },
                "last_check": { "type": "string", "format": "date-time" },
                "error_detail": { "type": "string", "nullable": true }
            }
        },
        "HealthComponents": {
            "type": "object",
            "required": ["database", "cron_scheduler", "alert_dispatcher", "gateways"],
            "properties": {
                "database": { "$ref": "#/components/schemas/ComponentHealth" },
                "cron_scheduler": { "$ref": "#/components/schemas/ComponentHealth" },
                "alert_dispatcher": { "$ref": "#/components/schemas/ComponentHealth" },
                "gateways": {
                    "type": "object",
                    "additionalProperties": { "$ref": "#/components/schemas/ComponentHealth" }
                }
            }
        },
        "HealthResponse": {
            "type": "object",
            "required": ["status", "version", "checked_at", "components"],
            "properties": {
                "status": { "$ref": "#/components/schemas/HealthStatus" },
                "version": { "type": "string", "example": "0.1.0" },
                "checked_at": { "type": "string", "format": "date-time" },
                "components": { "$ref": "#/components/schemas/HealthComponents" }
            }
        },
        "ServiceProbeResponse": {
            "type": "object",
            "required": ["status", "checked_at"],
            "properties": {
                "status": { "$ref": "#/components/schemas/HealthStatus" },
                "checked_at": { "type": "string", "format": "date-time" }
            }
        },
        "SessionMetrics": {
            "type": "object",
            "required": ["total", "messages"],
            "properties": {
                "total": { "type": "integer" },
                "messages": { "type": "integer" }
            }
        },
        "QueueMetrics": {
            "type": "object",
            "required": ["pending", "processing", "completed", "failed", "dead"],
            "properties": {
                "pending": { "type": "integer" },
                "processing": { "type": "integer" },
                "completed": { "type": "integer" },
                "failed": { "type": "integer" },
                "dead": { "type": "integer" }
            }
        },
        "RunMetrics": {
            "type": "object",
            "required": ["running", "completed", "failed", "suspended"],
            "properties": {
                "running": { "type": "integer" },
                "completed": { "type": "integer" },
                "failed": { "type": "integer" },
                "suspended": { "type": "integer" }
            }
        },
        "MetricsResponse": {
            "type": "object",
            "required": ["sessions", "queue", "runs"],
            "properties": {
                "sessions": { "$ref": "#/components/schemas/SessionMetrics" },
                "queue": { "$ref": "#/components/schemas/QueueMetrics" },
                "runs": { "$ref": "#/components/schemas/RunMetrics" }
            }
        }
    })
}
