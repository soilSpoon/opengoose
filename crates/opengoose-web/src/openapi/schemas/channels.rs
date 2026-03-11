use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "ChannelMetricsSnapshot": {
            "type": "object",
            "required": ["reconnect_count"],
            "properties": {
                "uptime_secs": { "type": "integer", "nullable": true },
                "reconnect_count": { "type": "integer" },
                "last_error": { "type": "string", "nullable": true }
            }
        },
        "ChannelMetricsResponse": {
            "type": "object",
            "required": ["platforms"],
            "description": "Per-platform channel adapter metrics. Keys are platform names.",
            "properties": {
                "platforms": {
                    "type": "object",
                    "additionalProperties": {
                        "$ref": "#/components/schemas/ChannelMetricsSnapshot"
                    }
                }
            }
        }
    })
}
