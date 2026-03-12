use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "TeamItem": {
            "type": "object",
            "required": ["name", "title", "workflow", "agent_count"],
            "properties": {
                "name": { "type": "string" },
                "title": { "type": "string" },
                "description": { "type": "string", "nullable": true },
                "workflow": {
                    "type": "string",
                    "enum": ["chain", "fan-out", "router"]
                },
                "agent_count": { "type": "integer" }
            }
        }
    })
}
