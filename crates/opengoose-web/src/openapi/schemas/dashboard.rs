use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "DashboardStats": {
            "type": "object",
            "required": ["session_count", "message_count", "run_count", "agent_count", "team_count"],
            "description": "Aggregate system statistics",
            "properties": {
                "session_count": { "type": "integer" },
                "message_count": { "type": "integer" },
                "run_count": { "type": "integer" },
                "agent_count": { "type": "integer" },
                "team_count": { "type": "integer" }
            }
        }
    })
}
