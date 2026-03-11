use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "GatewaySummary": {
            "type": "object",
            "required": ["platform", "state", "reconnect_count"],
            "properties": {
                "platform": { "type": "string" },
                "state": {
                    "type": "string",
                    "enum": ["connected", "disconnected", "reconnecting"]
                },
                "uptime_secs": { "type": "integer", "nullable": true },
                "reconnect_count": { "type": "integer" },
                "last_error": { "type": "string", "nullable": true }
            }
        },
        "GatewayListResponse": {
            "type": "object",
            "required": ["gateways"],
            "properties": {
                "gateways": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/GatewaySummary" }
                }
            }
        },
        "GatewayStatusResponse": {
            "type": "object",
            "required": ["platform", "state", "reconnect_count"],
            "properties": {
                "platform": { "type": "string" },
                "state": {
                    "type": "string",
                    "enum": ["connected", "disconnected", "reconnecting"]
                },
                "uptime_secs": { "type": "integer", "nullable": true },
                "reconnect_count": { "type": "integer" },
                "last_error": { "type": "string", "nullable": true }
            }
        }
    })
}
