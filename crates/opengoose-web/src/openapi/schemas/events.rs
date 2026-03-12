use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "EventHistoryResponse": {
            "type": "object",
            "required": ["id", "event_kind", "timestamp", "payload"],
            "properties": {
                "id": { "type": "integer" },
                "event_kind": { "type": "string" },
                "timestamp": { "type": "string", "format": "date-time" },
                "source_gateway": { "type": "string", "nullable": true },
                "session_key": { "type": "string", "nullable": true },
                "payload": {
                    "type": "object",
                    "description": "Tagged JSON payload for the persisted app event"
                }
            }
        },
        "EventHistoryPageResponse": {
            "type": "object",
            "required": ["items", "limit", "offset", "has_more"],
            "properties": {
                "items": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/EventHistoryResponse" }
                },
                "limit": { "type": "integer" },
                "offset": { "type": "integer" },
                "has_more": { "type": "boolean" }
            }
        }
    })
}
