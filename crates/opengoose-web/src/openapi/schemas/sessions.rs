use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "SessionItem": {
            "type": "object",
            "required": ["session_key", "created_at", "updated_at"],
            "properties": {
                "session_key": { "type": "string" },
                "active_team": { "type": "string", "nullable": true },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" }
            }
        },
        "MessageItem": {
            "type": "object",
            "required": ["role", "content", "created_at"],
            "properties": {
                "role": { "type": "string", "enum": ["user", "assistant", "system"] },
                "content": { "type": "string" },
                "author": { "type": "string", "nullable": true },
                "created_at": { "type": "string", "format": "date-time" }
            }
        }
    })
}
