use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "AlertRuleResponse": {
            "type": "object",
            "required": ["id", "name", "metric", "condition", "threshold", "enabled",
                         "created_at", "updated_at"],
            "properties": {
                "id": { "type": "string" },
                "name": { "type": "string" },
                "description": { "type": "string", "nullable": true },
                "metric": {
                    "type": "string",
                    "enum": ["queue_backlog", "failed_runs", "error_rate"]
                },
                "condition": {
                    "type": "string",
                    "enum": ["gt", "lt", "gte", "lte"]
                },
                "threshold": { "type": "number", "format": "double" },
                "enabled": { "type": "boolean" },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" }
            }
        },
        "AlertHistoryResponse": {
            "type": "object",
            "required": ["id", "rule_id", "rule_name", "metric", "value", "triggered_at"],
            "properties": {
                "id": { "type": "integer" },
                "rule_id": { "type": "string" },
                "rule_name": { "type": "string" },
                "metric": { "type": "string" },
                "value": { "type": "number", "format": "double" },
                "triggered_at": { "type": "string", "format": "date-time" }
            }
        },
        "CreateAlertRequest": {
            "type": "object",
            "required": ["name", "metric", "condition", "threshold"],
            "properties": {
                "name": { "type": "string" },
                "description": { "type": "string", "nullable": true },
                "metric": {
                    "type": "string",
                    "enum": ["queue_backlog", "failed_runs", "error_rate"]
                },
                "condition": {
                    "type": "string",
                    "enum": ["gt", "lt", "gte", "lte"]
                },
                "threshold": { "type": "number", "format": "double" }
            }
        },
        "TestAlertsResponse": {
            "type": "object",
            "required": ["evaluated", "triggered"],
            "properties": {
                "evaluated": { "type": "integer" },
                "triggered": { "type": "integer" }
            }
        }
    })
}
