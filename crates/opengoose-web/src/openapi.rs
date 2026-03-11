/// OpenAPI 3.0 specification for the OpenGoose web dashboard JSON API.
///
/// Served as JSON at `GET /api/openapi.json`.
/// An embedded Swagger UI is available at `GET /api/docs`.
use axum::http::header;
use axum::response::{Html, IntoResponse};
use serde_json::{Value, json};

/// Build the complete OpenAPI 3.0 spec as a JSON value.
pub fn build_spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "OpenGoose Web API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "JSON API for the OpenGoose autonomous agent orchestration dashboard. \
                Provides session management, run history, workflow control, alert rules, \
                trigger management, gateway status, and real-time SSE events.",
            "contact": {
                "name": "OpenGoose",
                "url": "https://github.com/soilSpoon/opengoose"
            },
            "license": {
                "name": "MIT"
            }
        },
        "tags": [
            { "name": "system", "description": "Health and metrics endpoints" },
            { "name": "dashboard", "description": "Aggregate dashboard statistics" },
            { "name": "sessions", "description": "Chat session and message management" },
            { "name": "runs", "description": "Orchestration run history" },
            { "name": "agents", "description": "Agent profile management" },
            { "name": "teams", "description": "Team definition management" },
            { "name": "workflows", "description": "Workflow definitions and manual triggers" },
            { "name": "alerts", "description": "Alert rule management and history" },
            { "name": "triggers", "description": "Trigger CRUD and test-fire operations" },
            { "name": "channels", "description": "Channel adapter metrics" },
            { "name": "gateways", "description": "Gateway platform health status" },
            { "name": "webhooks", "description": "Inbound webhook receiver" },
            { "name": "events", "description": "Server-Sent Events stream" }
        ],
        "paths": {
            "/api/health": {
                "get": {
                    "tags": ["system"],
                    "summary": "Health check",
                    "operationId": "getHealth",
                    "responses": {
                        "200": {
                            "description": "Service is healthy",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/HealthResponse" }
                                }
                            }
                        }
                    }
                }
            },
            "/api/metrics": {
                "get": {
                    "tags": ["system"],
                    "summary": "System metrics",
                    "operationId": "getMetrics",
                    "responses": {
                        "200": {
                            "description": "Current system metrics",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/MetricsResponse" }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/dashboard": {
                "get": {
                    "tags": ["dashboard"],
                    "summary": "Dashboard statistics",
                    "operationId": "getDashboard",
                    "responses": {
                        "200": {
                            "description": "Aggregate system statistics",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/DashboardStats" }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/sessions": {
                "get": {
                    "tags": ["sessions"],
                    "summary": "List sessions",
                    "operationId": "listSessions",
                    "parameters": [
                        {
                            "name": "limit",
                            "in": "query",
                            "description": "Max records to return (default 50, max 1000)",
                            "schema": { "type": "integer", "default": 50 }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of chat sessions",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/SessionItem" }
                                    }
                                }
                            }
                        },
                        "422": { "$ref": "#/components/responses/UnprocessableEntity" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/sessions/{session_key}/messages": {
                "get": {
                    "tags": ["sessions"],
                    "summary": "Get messages for a session",
                    "operationId": "getSessionMessages",
                    "parameters": [
                        {
                            "name": "session_key",
                            "in": "path",
                            "required": true,
                            "description": "Session key identifier",
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "description": "Max records to return (default 100, max 5000)",
                            "schema": { "type": "integer", "default": 100 }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of messages",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/MessageItem" }
                                    }
                                }
                            }
                        },
                        "404": { "$ref": "#/components/responses/NotFound" },
                        "422": { "$ref": "#/components/responses/UnprocessableEntity" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/runs": {
                "get": {
                    "tags": ["runs"],
                    "summary": "List orchestration runs",
                    "operationId": "listRuns",
                    "parameters": [
                        {
                            "name": "status",
                            "in": "query",
                            "description": "Filter by status",
                            "schema": {
                                "type": "string",
                                "enum": ["running", "completed", "failed", "suspended"]
                            }
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "description": "Max records to return (default 50, max 1000)",
                            "schema": { "type": "integer", "default": 50 }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of orchestration runs",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/RunItem" }
                                    }
                                }
                            }
                        },
                        "422": { "$ref": "#/components/responses/UnprocessableEntity" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/agents": {
                "get": {
                    "tags": ["agents"],
                    "summary": "List agent profiles",
                    "operationId": "listAgents",
                    "responses": {
                        "200": {
                            "description": "List of installed agent profiles",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/AgentItem" }
                                    }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/teams": {
                "get": {
                    "tags": ["teams"],
                    "summary": "List team definitions",
                    "operationId": "listTeams",
                    "responses": {
                        "200": {
                            "description": "List of team definitions",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/TeamItem" }
                                    }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/workflows": {
                "get": {
                    "tags": ["workflows"],
                    "summary": "List workflow definitions",
                    "operationId": "listWorkflows",
                    "responses": {
                        "200": {
                            "description": "List of workflow definitions with automation summaries",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/WorkflowItem" }
                                    }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/workflows/{name}": {
                "get": {
                    "tags": ["workflows"],
                    "summary": "Get workflow detail",
                    "operationId": "getWorkflow",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "description": "Workflow name",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Workflow detail with steps, automations, and recent runs",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/WorkflowDetail" }
                                }
                            }
                        },
                        "404": { "$ref": "#/components/responses/NotFound" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/workflows/{name}/trigger": {
                "post": {
                    "tags": ["workflows"],
                    "summary": "Trigger a workflow manually",
                    "operationId": "triggerWorkflow",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "description": "Workflow name",
                            "schema": { "type": "string" }
                        }
                    ],
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/TriggerWorkflowRequest" }
                            }
                        }
                    },
                    "responses": {
                        "202": {
                            "description": "Accepted — workflow enqueued for execution",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TriggerWorkflowResponse" }
                                }
                            }
                        },
                        "404": { "$ref": "#/components/responses/NotFound" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/alerts": {
                "get": {
                    "tags": ["alerts"],
                    "summary": "List alert rules",
                    "operationId": "listAlerts",
                    "responses": {
                        "200": {
                            "description": "List of alert rules",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/AlertRuleResponse" }
                                    }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                },
                "post": {
                    "tags": ["alerts"],
                    "summary": "Create alert rule",
                    "operationId": "createAlert",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateAlertRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Created alert rule",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/AlertRuleResponse" }
                                }
                            }
                        },
                        "422": { "$ref": "#/components/responses/UnprocessableEntity" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/alerts/history": {
                "get": {
                    "tags": ["alerts"],
                    "summary": "List alert trigger history",
                    "operationId": "getAlertHistory",
                    "responses": {
                        "200": {
                            "description": "List of alert trigger events",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/AlertHistoryResponse" }
                                    }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/alerts/test": {
                "post": {
                    "tags": ["alerts"],
                    "summary": "Evaluate all alert rules immediately",
                    "operationId": "testAlerts",
                    "responses": {
                        "200": {
                            "description": "Evaluation summary",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TestAlertsResponse" }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/alerts/{name}": {
                "delete": {
                    "tags": ["alerts"],
                    "summary": "Delete alert rule",
                    "operationId": "deleteAlert",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "description": "Alert rule name",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "204": { "description": "Deleted successfully" },
                        "404": { "$ref": "#/components/responses/NotFound" }
                    }
                }
            },
            "/api/triggers": {
                "get": {
                    "tags": ["triggers"],
                    "summary": "List triggers",
                    "operationId": "listTriggers",
                    "responses": {
                        "200": {
                            "description": "List of triggers",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": { "$ref": "#/components/schemas/TriggerItem" }
                                    }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                },
                "post": {
                    "tags": ["triggers"],
                    "summary": "Create trigger",
                    "operationId": "createTrigger",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CreateTriggerRequest" }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Created trigger",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TriggerItem" }
                                }
                            }
                        },
                        "422": { "$ref": "#/components/responses/UnprocessableEntity" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/triggers/{name}": {
                "get": {
                    "tags": ["triggers"],
                    "summary": "Get trigger",
                    "operationId": "getTrigger",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "description": "Trigger name",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Trigger detail",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TriggerItem" }
                                }
                            }
                        },
                        "404": { "$ref": "#/components/responses/NotFound" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                },
                "put": {
                    "tags": ["triggers"],
                    "summary": "Update trigger",
                    "operationId": "updateTrigger",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "description": "Trigger name",
                            "schema": { "type": "string" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/UpdateTriggerRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Updated trigger",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TriggerItem" }
                                }
                            }
                        },
                        "404": { "$ref": "#/components/responses/NotFound" },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                },
                "delete": {
                    "tags": ["triggers"],
                    "summary": "Delete trigger",
                    "operationId": "deleteTrigger",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "description": "Trigger name",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "204": { "description": "Deleted successfully" },
                        "404": { "$ref": "#/components/responses/NotFound" }
                    }
                }
            },
            "/api/triggers/{name}/enabled": {
                "patch": {
                    "tags": ["triggers"],
                    "summary": "Enable or disable trigger",
                    "operationId": "setTriggerEnabled",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "description": "Trigger name",
                            "schema": { "type": "string" }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["enabled"],
                                    "properties": {
                                        "enabled": { "type": "boolean" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Updated successfully" },
                        "404": { "$ref": "#/components/responses/NotFound" }
                    }
                }
            },
            "/api/triggers/{name}/test": {
                "post": {
                    "tags": ["triggers"],
                    "summary": "Fire a test event for a trigger",
                    "operationId": "testTrigger",
                    "parameters": [
                        {
                            "name": "name",
                            "in": "path",
                            "required": true,
                            "description": "Trigger name",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Test result",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/TriggerTestResponse" }
                                }
                            }
                        },
                        "404": { "$ref": "#/components/responses/NotFound" }
                    }
                }
            },
            "/api/channel-metrics": {
                "get": {
                    "tags": ["channels"],
                    "summary": "Channel adapter metrics",
                    "operationId": "getChannelMetrics",
                    "responses": {
                        "200": {
                            "description": "Per-platform channel adapter metrics",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ChannelMetricsResponse" }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/gateways": {
                "get": {
                    "tags": ["gateways"],
                    "summary": "List gateway platforms",
                    "operationId": "listGateways",
                    "responses": {
                        "200": {
                            "description": "Connection status for all known gateway platforms",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/GatewayListResponse" }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/gateways/{platform}/status": {
                "get": {
                    "tags": ["gateways"],
                    "summary": "Gateway platform status",
                    "operationId": "getGatewayStatus",
                    "parameters": [
                        {
                            "name": "platform",
                            "in": "path",
                            "required": true,
                            "description": "Platform name (e.g. discord, slack, telegram, matrix)",
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Detailed status for the specified platform",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/GatewayStatusResponse" }
                                }
                            }
                        },
                        "500": { "$ref": "#/components/responses/InternalError" }
                    }
                }
            },
            "/api/webhooks/{path}": {
                "post": {
                    "tags": ["webhooks"],
                    "summary": "Receive inbound webhook",
                    "description": "Receives an HTTP request and fires any matching webhook triggers.",
                    "operationId": "receiveWebhook",
                    "parameters": [
                        {
                            "name": "path",
                            "in": "path",
                            "required": true,
                            "description": "Webhook path to match against registered triggers",
                            "schema": { "type": "string" }
                        }
                    ],
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": { "type": "object" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Webhook accepted and matching trigger fired" },
                        "404": { "description": "No matching trigger found for this path" }
                    }
                }
            },
            "/api/events": {
                "get": {
                    "tags": ["events"],
                    "summary": "Server-Sent Events stream",
                    "description": "Streams text/event-stream events for live dashboard updates. \
                        Event types: session, run, queue.",
                    "operationId": "streamEvents",
                    "responses": {
                        "200": {
                            "description": "SSE event stream",
                            "content": {
                                "text/event-stream": {
                                    "schema": { "type": "string" }
                                }
                            }
                        }
                    }
                }
            }
        },
        "components": {
            "responses": {
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
            },
            "schemas": {
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
                },
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
            }
        }
    })
}

/// Swagger UI HTML page that loads the spec from `/api/openapi.json`.
const SWAGGER_UI_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
  <title>OpenGoose API — Swagger UI</title>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({
      url: "/api/openapi.json",
      dom_id: "#swagger-ui",
      presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
      layout: "BaseLayout",
      deepLinking: true,
    });
  </script>
</body>
</html>"##;

/// `GET /api/openapi.json` — serve the OpenAPI 3.0 spec as JSON.
pub async fn serve_openapi_json() -> impl IntoResponse {
    let spec = build_spec();
    let json = serde_json::to_string_pretty(&spec).unwrap_or_else(|_| "{}".to_string());
    ([(header::CONTENT_TYPE, "application/json")], json)
}

/// `GET /api/docs` — serve the Swagger UI HTML page.
pub async fn serve_swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_valid_json() {
        let spec = build_spec();
        let json = serde_json::to_string(&spec).expect("spec should serialize to JSON");
        assert!(json.contains("openapi"));
        assert!(json.contains("OpenGoose Web API"));
    }

    #[test]
    fn spec_contains_all_api_paths() {
        let spec = build_spec();
        let paths = spec["paths"].as_object().expect("paths should be an object");

        let expected = [
            "/api/health",
            "/api/metrics",
            "/api/dashboard",
            "/api/sessions",
            "/api/sessions/{session_key}/messages",
            "/api/runs",
            "/api/agents",
            "/api/teams",
            "/api/workflows",
            "/api/workflows/{name}",
            "/api/workflows/{name}/trigger",
            "/api/alerts",
            "/api/alerts/history",
            "/api/alerts/test",
            "/api/alerts/{name}",
            "/api/triggers",
            "/api/triggers/{name}",
            "/api/triggers/{name}/enabled",
            "/api/triggers/{name}/test",
            "/api/channel-metrics",
            "/api/gateways",
            "/api/gateways/{platform}/status",
            "/api/webhooks/{path}",
            "/api/events",
        ];

        for path in &expected {
            assert!(paths.contains_key(*path), "missing path: {path}");
        }
    }

    #[test]
    fn spec_contains_schema_components() {
        let spec = build_spec();
        let schemas = spec["components"]["schemas"]
            .as_object()
            .expect("schemas should be an object");

        let expected_schemas = [
            "ErrorResponse",
            "HealthResponse",
            "DashboardStats",
            "SessionItem",
            "MessageItem",
            "RunItem",
            "AgentItem",
            "TeamItem",
            "WorkflowItem",
            "WorkflowDetail",
            "AlertRuleResponse",
            "CreateAlertRequest",
            "TriggerItem",
            "GatewaySummary",
            "GatewayListResponse",
        ];

        for schema in &expected_schemas {
            assert!(schemas.contains_key(*schema), "missing schema: {schema}");
        }
    }

    #[test]
    fn spec_has_correct_openapi_version() {
        let spec = build_spec();
        assert_eq!(spec["openapi"], "3.0.3");
    }

    #[tokio::test]
    async fn serve_openapi_json_returns_content_type_json() {
        use axum::response::IntoResponse;
        let response = serve_openapi_json().await.into_response();
        let content_type = response
            .headers()
            .get("content-type")
            .expect("content-type should be set")
            .to_str()
            .expect("content-type should be valid string");
        assert!(content_type.contains("application/json"));
    }

    #[tokio::test]
    async fn serve_swagger_ui_returns_html_with_swagger_ui() {
        let html = serve_swagger_ui().await;
        assert!(html.0.contains("swagger-ui"));
        assert!(html.0.contains("/api/openapi.json"));
    }
}
