use serde_json::{Value, json};

/// Common reusable response definitions (NotFound, InternalError, UnprocessableEntity).
pub(super) fn common_responses() -> Value {
    json!({
        "NotFound": {
            "description": "Resource not found",
            "content": {
                "application/json": {
                    "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                }
            }
        },
        "InternalError": {
            "description": "Internal server error",
            "content": {
                "application/json": {
                    "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                }
            }
        },
        "UnprocessableEntity": {
            "description": "Unprocessable entity — validation failed",
            "content": {
                "application/json": {
                    "schema": { "$ref": "#/components/schemas/ErrorResponse" }
                }
            }
        }
    })
}

/// Build all component schema definitions by merging core and operations schemas.
pub(super) fn build_schemas() -> Value {
    let mut schemas = core_schemas().as_object().cloned().unwrap_or_default();
    if let Some(ops) = ops_schemas().as_object() {
        schemas.extend(ops.clone());
    }
    Value::Object(schemas)
}

/// Core data model schemas: errors, health, metrics, dashboard, sessions, runs, agents, teams.
fn core_schemas() -> Value {
    json!({
        "ErrorResponse": {
            "type": "object",
            "required": ["error"],
            "properties": {
                "error": { "type": "string", "description": "Human-readable error message" }
            }
        },
        "HealthResponse": {
            "type": "object",
            "required": ["status", "version"],
            "properties": {
                "status": { "type": "string", "example": "ok" },
                "version": { "type": "string", "example": "0.1.0" }
            }
        },
        "SessionMetrics": {
            "type": "object",
            "required": ["total", "messages"],
            "properties": {
                "total": { "type": "integer" },
                "messages": { "type": "integer" }
            }
        },
        "QueueMetrics": {
            "type": "object",
            "required": ["pending", "processing", "completed", "failed", "dead"],
            "properties": {
                "pending": { "type": "integer" },
                "processing": { "type": "integer" },
                "completed": { "type": "integer" },
                "failed": { "type": "integer" },
                "dead": { "type": "integer" }
            }
        },
        "RunMetrics": {
            "type": "object",
            "required": ["running", "completed", "failed", "suspended"],
            "properties": {
                "running": { "type": "integer" },
                "completed": { "type": "integer" },
                "failed": { "type": "integer" },
                "suspended": { "type": "integer" }
            }
        },
        "MetricsResponse": {
            "type": "object",
            "required": ["sessions", "queue", "runs"],
            "properties": {
                "sessions": { "$ref": "#/components/schemas/SessionMetrics" },
                "queue": { "$ref": "#/components/schemas/QueueMetrics" },
                "runs": { "$ref": "#/components/schemas/RunMetrics" }
            }
        },
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
        },
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
        },
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
        },
        "AgentItem": {
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string", "description": "Profile name" }
            }
        },
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

/// Operations schemas: workflows, alerts, triggers, channels, gateways.
fn ops_schemas() -> Value {
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
        },
        "AlertRuleResponse": {
            "type": "object",
            "required": ["id", "name", "metric", "condition", "threshold", "enabled",
                         "created_at", "updated_at"],
            "properties": {
                "id": { "type": "string" },
                "name": { "type": "string" },
                "description": { "type": "string", "nullable": true },
                "metric": {
                    "type": "string",
                    "enum": ["queue_backlog", "failed_runs", "error_rate"]
                },
                "condition": {
                    "type": "string",
                    "enum": ["gt", "lt", "gte", "lte"]
                },
                "threshold": { "type": "number", "format": "double" },
                "enabled": { "type": "boolean" },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" }
            }
        },
        "AlertHistoryResponse": {
            "type": "object",
            "required": ["id", "rule_id", "rule_name", "metric", "value", "triggered_at"],
            "properties": {
                "id": { "type": "integer" },
                "rule_id": { "type": "string" },
                "rule_name": { "type": "string" },
                "metric": { "type": "string" },
                "value": { "type": "number", "format": "double" },
                "triggered_at": { "type": "string", "format": "date-time" }
            }
        },
        "CreateAlertRequest": {
            "type": "object",
            "required": ["name", "metric", "condition", "threshold"],
            "properties": {
                "name": { "type": "string" },
                "description": { "type": "string", "nullable": true },
                "metric": {
                    "type": "string",
                    "enum": ["queue_backlog", "failed_runs", "error_rate"]
                },
                "condition": {
                    "type": "string",
                    "enum": ["gt", "lt", "gte", "lte"]
                },
                "threshold": { "type": "number", "format": "double" }
            }
        },
        "TestAlertsResponse": {
            "type": "object",
            "required": ["evaluated", "triggered"],
            "properties": {
                "evaluated": { "type": "integer" },
                "triggered": { "type": "integer" }
            }
        },
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
        },
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
        },
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
