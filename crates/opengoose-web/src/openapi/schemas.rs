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
        "HealthStatus": {
            "type": "string",
            "enum": ["healthy", "degraded", "unavailable"]
        },
        "ComponentHealth": {
            "type": "object",
            "required": ["status", "last_check"],
            "properties": {
                "status": { "$ref": "#/components/schemas/HealthStatus" },
                "last_check": { "type": "string", "format": "date-time" },
                "error_detail": { "type": "string", "nullable": true }
            }
        },
        "HealthComponents": {
            "type": "object",
            "required": ["database", "cron_scheduler", "alert_dispatcher", "gateways"],
            "properties": {
                "database": { "$ref": "#/components/schemas/ComponentHealth" },
                "cron_scheduler": { "$ref": "#/components/schemas/ComponentHealth" },
                "alert_dispatcher": { "$ref": "#/components/schemas/ComponentHealth" },
                "gateways": {
                    "type": "object",
                    "additionalProperties": { "$ref": "#/components/schemas/ComponentHealth" }
                }
            }
        },
        "HealthResponse": {
            "type": "object",
            "required": ["status", "version", "checked_at", "components"],
            "properties": {
                "status": { "$ref": "#/components/schemas/HealthStatus" },
                "version": { "type": "string", "example": "0.1.0" },
                "checked_at": { "type": "string", "format": "date-time" },
                "components": { "$ref": "#/components/schemas/HealthComponents" }
            }
        },
        "ServiceProbeResponse": {
            "type": "object",
            "required": ["status", "checked_at"],
            "properties": {
                "status": { "$ref": "#/components/schemas/HealthStatus" },
                "checked_at": { "type": "string", "format": "date-time" }
            }
        },
        "SessionMetrics": {
            "type": "object",
            "required": [
                "total",
                "messages",
                "estimated_tokens",
                "active",
                "active_window_minutes",
                "average_duration_seconds",
                "per_session"
            ],
            "properties": {
                "total": { "type": "integer" },
                "messages": { "type": "integer" },
                "estimated_tokens": {
                    "type": "integer",
                    "description": "Approximate token usage across all persisted session messages, estimated at roughly 4 characters per token."
                },
                "active": {
                    "type": "integer",
                    "description": "Sessions with activity inside the active window."
                },
                "active_window_minutes": {
                    "type": "integer",
                    "description": "Rolling activity window used to classify a session as active."
                },
                "average_duration_seconds": {
                    "type": "number",
                    "format": "double"
                },
                "per_session": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/SessionMetricsItem" }
                }
            }
        },
        "SessionMetricsItem": {
            "type": "object",
            "required": [
                "session_key",
                "created_at",
                "updated_at",
                "message_count",
                "estimated_tokens",
                "duration_seconds",
                "active"
            ],
            "properties": {
                "session_key": { "type": "string" },
                "active_team": { "type": "string", "nullable": true },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" },
                "message_count": { "type": "integer" },
                "estimated_tokens": {
                    "type": "integer",
                    "description": "Approximate token usage for this session, estimated at roughly 4 characters per token."
                },
                "duration_seconds": { "type": "integer" },
                "active": { "type": "boolean" }
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
        "SessionExport": {
            "type": "object",
            "required": [
                "session_key",
                "created_at",
                "updated_at",
                "message_count",
                "messages"
            ],
            "properties": {
                "session_key": { "type": "string" },
                "active_team": { "type": "string", "nullable": true },
                "created_at": { "type": "string", "format": "date-time" },
                "updated_at": { "type": "string", "format": "date-time" },
                "message_count": { "type": "integer" },
                "messages": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/MessageItem" }
                }
            }
        },
        "SessionBatchExport": {
            "type": "object",
            "required": ["session_count", "sessions"],
            "properties": {
                "since": { "type": "string", "nullable": true },
                "until": { "type": "string", "nullable": true },
                "session_count": { "type": "integer" },
                "sessions": {
                    "type": "array",
                    "items": { "$ref": "#/components/schemas/SessionExport" }
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_ref(value: &Value) -> Option<&str> {
        value["content"]["application/json"]["schema"]["$ref"].as_str()
    }

    #[test]
    fn common_responses_wrap_error_response_schema() {
        let responses = common_responses();
        let responses = responses
            .as_object()
            .expect("common responses should be an object");

        assert_eq!(responses.len(), 3);

        for (name, description) in [
            ("NotFound", "Resource not found"),
            ("InternalError", "Internal server error"),
            (
                "UnprocessableEntity",
                "Unprocessable entity — validation failed",
            ),
        ] {
            let response = responses
                .get(name)
                .unwrap_or_else(|| panic!("missing response {name}"));
            assert_eq!(response["description"], description);
            assert_eq!(
                schema_ref(response),
                Some("#/components/schemas/ErrorResponse")
            );
        }
    }

    #[test]
    fn build_schemas_merges_core_and_ops_without_dropping_entries() {
        let core = core_schemas();
        let ops = ops_schemas();
        let merged = build_schemas();

        let core = core.as_object().expect("core schemas should be an object");
        let ops = ops.as_object().expect("ops schemas should be an object");
        let merged = merged
            .as_object()
            .expect("merged schemas should be an object");

        assert_eq!(merged.len(), core.len() + ops.len());

        for key in core.keys().chain(ops.keys()) {
            assert!(merged.contains_key(key), "missing merged schema {key}");
        }
    }

    #[test]
    fn core_schemas_capture_required_fields_and_enums() {
        let schemas = core_schemas();
        let schemas = schemas
            .as_object()
            .expect("core schemas should be an object");

        assert_eq!(
            schemas["HealthResponse"]["required"],
            json!(["status", "version", "checked_at", "components"])
        );
        assert_eq!(
            schemas["HealthStatus"]["enum"],
            json!(["healthy", "degraded", "unavailable"])
        );
        assert_eq!(
            schemas["HealthComponents"]["required"],
            json!(["database", "cron_scheduler", "alert_dispatcher", "gateways"])
        );
        assert_eq!(
            schemas["SessionItem"]["properties"]["active_team"]["nullable"],
            true
        );
        assert_eq!(
            schemas["SessionMetrics"]["properties"]["per_session"]["items"]["$ref"],
            "#/components/schemas/SessionMetricsItem"
        );
        assert_eq!(
            schemas["SessionMetricsItem"]["properties"]["active"]["type"],
            "boolean"
        );
        assert_eq!(
            schemas["MessageItem"]["properties"]["role"]["enum"],
            json!(["user", "assistant", "system"])
        );
        assert_eq!(
            schemas["SessionExport"]["properties"]["messages"]["items"]["$ref"],
            "#/components/schemas/MessageItem"
        );
        assert_eq!(
            schemas["SessionBatchExport"]["properties"]["sessions"]["items"]["$ref"],
            "#/components/schemas/SessionExport"
        );
        assert_eq!(
            schemas["TeamItem"]["properties"]["workflow"]["enum"],
            json!(["chain", "fan-out", "router"])
        );
        assert_eq!(
            schemas["RunItem"]["properties"]["created_at"]["format"],
            "date-time"
        );
    }

    #[test]
    fn ops_schemas_define_expected_enums_and_nested_refs() {
        let schemas = ops_schemas();
        let schemas = schemas
            .as_object()
            .expect("ops schemas should be an object");

        assert_eq!(
            schemas["WorkflowAutomation"]["properties"]["kind"]["enum"],
            json!(["schedule", "trigger"])
        );
        assert_eq!(
            schemas["AlertRuleResponse"]["properties"]["metric"]["enum"],
            json!(["queue_backlog", "failed_runs", "error_rate"])
        );
        assert_eq!(
            schemas["ChannelMetricsResponse"]["properties"]["platforms"]["additionalProperties"]["$ref"],
            "#/components/schemas/ChannelMetricsSnapshot"
        );
        assert_eq!(
            schemas["GatewayListResponse"]["properties"]["gateways"]["items"]["$ref"],
            "#/components/schemas/GatewaySummary"
        );
        assert_eq!(
            schemas["TriggerWorkflowResponse"]["required"],
            json!(["workflow", "accepted", "input"])
        );
        assert_eq!(
            schemas["TriggerTestResponse"]["properties"]["fired"]["type"],
            "boolean"
        );
    }

    #[test]
    fn every_required_field_is_declared_in_schema_properties() {
        let schemas = build_schemas();
        let schemas = schemas
            .as_object()
            .expect("merged schemas should be an object");

        for (name, schema) in schemas {
            if schema["type"] != "object" {
                continue;
            }
            let properties = schema["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("schema {name} should declare properties"));

            if let Some(required) = schema["required"].as_array() {
                for field in required {
                    let field = field
                        .as_str()
                        .unwrap_or_else(|| panic!("schema {name} required fields must be strings"));
                    assert!(
                        properties.contains_key(field),
                        "schema {name} is missing property {field}"
                    );
                }
            }
        }
    }
}
