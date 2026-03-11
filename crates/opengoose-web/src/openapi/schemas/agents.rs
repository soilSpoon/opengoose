use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "AgentItem": {
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string", "description": "Profile name" }
            }
        }
    })
}
