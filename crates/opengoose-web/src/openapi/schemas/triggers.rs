use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "TriggerItem": {
            "type": "object",
            "required": ["name", "trigger_type", "team_name", "enabled", "fire_count",
                         "created_at", "updated_at"],
            "properties": {
                "name": { "type": "string" },
                "trigger_type": { "type": "string" },
                "team_name": { "type": "string" },
                "enabled": { "type": "boolean" },
                "fire_count": { "type": "integer" },
                "last_fired_at": {
                    "type": "string",
                    "nullable": true,
                    "format": "date-time"
                },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" }
            }
        },
        "CreateTriggerRequest": {
            "type": "object",
            "required": ["name", "trigger_type", "config", "team_name"],
            "properties": {
                "name": { "type": "string" },
                "trigger_type": { "type": "string" },
                "config": {
                    "type": "string",
                    "description": "JSON configuration for the trigger"
                },
                "team_name": { "type": "string" },
                "description": { "type": "string", "nullable": true }
            }
        },
        "UpdateTriggerRequest": {
            "type": "object",
            "properties": {
                "trigger_type": { "type": "string", "nullable": true },
                "config": { "type": "string", "nullable": true },
                "team_name": { "type": "string", "nullable": true },
                "description": { "type": "string", "nullable": true }
            }
        },
        "TriggerTestResponse": {
            "type": "object",
            "required": ["fired", "trigger_name"],
            "properties": {
                "fired": { "type": "boolean" },
                "trigger_name": { "type": "string" }
            }
        }
    })
}
