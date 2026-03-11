use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "RunItem": {
            "type": "object",
            "required": ["team_run_id", "session_key", "team_name", "workflow", "status",
                         "current_step", "total_steps", "created_at", "updated_at"],
            "properties": {
                "team_run_id": { "type": "string" },
                "session_key": { "type": "string" },
                "team_name": { "type": "string" },
                "workflow": { "type": "string" },
                "status": { "type": "string" },
                "current_step": { "type": "integer" },
                "total_steps": { "type": "integer" },
                "result": { "type": "string", "nullable": true },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" }
            }
        }
    })
}
