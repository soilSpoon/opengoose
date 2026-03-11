use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
        "WorkflowStep": {
            "type": "object",
            "required": ["profile"],
            "properties": {
                "profile": { "type": "string" },
                "role": { "type": "string", "nullable": true }
            }
        },
        "WorkflowAutomation": {
            "type": "object",
            "required": ["kind", "name", "enabled", "detail", "note"],
            "properties": {
                "kind": { "type": "string", "enum": ["schedule", "trigger"] },
                "name": { "type": "string" },
                "enabled": { "type": "boolean" },
                "detail": { "type": "string" },
                "note": { "type": "string" }
            }
        },
        "WorkflowRun": {
            "type": "object",
            "required": ["team_run_id", "status", "current_step", "total_steps", "updated_at"],
            "properties": {
                "team_run_id": { "type": "string" },
                "status": { "type": "string" },
                "current_step": { "type": "integer" },
                "total_steps": { "type": "integer" },
                "updated_at": { "type": "string", "format": "date-time" }
            }
        },
        "WorkflowItem": {
            "type": "object",
            "required": ["name", "title", "workflow", "agent_count",
                         "schedule_count", "enabled_schedule_count",
                         "trigger_count", "enabled_trigger_count"],
            "properties": {
                "name": { "type": "string" },
                "title": { "type": "string" },
                "description": { "type": "string", "nullable": true },
                "workflow": { "type": "string" },
                "agent_count": { "type": "integer" },
                "schedule_count": { "type": "integer" },
                "enabled_schedule_count": { "type": "integer" },
                "trigger_count": { "type": "integer" },
                "enabled_trigger_count": { "type": "integer" },
                "last_run_status": { "type": "string", "nullable": true },
                "last_run_at": { "type": "string", "nullable": true, "format": "date-time" }
            }
        },
        "WorkflowDetail": {
            "type": "object",
            "required": ["name", "title", "workflow", "source_label", "yaml", "steps",
                         "automations", "recent_runs"],
            "properties": {
                "name": { "type": "string" },
                "title": { "type": "string" },
                "description": { "type": "string", "nullable": true },
                "workflow": { "type": "string" },
                "source_label": { "type": "string" },
                "yaml": { "type": "string" },
                "steps": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/WorkflowStep" }
                },
                "automations": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/WorkflowAutomation" }
                },
                "recent_runs": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/WorkflowRun" }
                }
            }
        },
        "TriggerWorkflowRequest": {
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "nullable": true,
                    "description": "Optional task input. Defaults to a standard message if omitted."
                }
            }
        },
        "TriggerWorkflowResponse": {
            "type": "object",
            "required": ["workflow", "accepted", "input"],
            "properties": {
                "workflow": { "type": "string" },
                "accepted": { "type": "boolean" },
                "input": { "type": "string" }
            }
        }
    })
}
