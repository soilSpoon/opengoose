/// Core API paths: system, dashboard, sessions, runs, agents, teams, workflows.
use serde_json::{Value, json};

pub(in crate::openapi) fn build() -> Value {
    json!({
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
        }
    })
}
