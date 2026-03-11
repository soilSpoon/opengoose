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
                        "description": "Current service health snapshot",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/HealthResponse" }
                            }
                        }
                    }
                }
            }
        },
        "/api/health/ready": {
            "get": {
                "tags": ["system"],
                "summary": "Readiness probe",
                "operationId": "getReadiness",
                "responses": {
                    "200": {
                        "description": "All critical components are healthy",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/HealthResponse" }
                            }
                        }
                    },
                    "503": {
                        "description": "One or more critical components are not healthy",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/HealthResponse" }
                            }
                        }
                    }
                }
            }
        },
        "/api/health/live": {
            "get": {
                "tags": ["system"],
                "summary": "Liveness probe",
                "operationId": "getLiveness",
                "responses": {
                    "200": {
                        "description": "Process is running",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ServiceProbeResponse" }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn operation<'a>(paths: &'a Value, path: &str, method: &str) -> &'a Value {
        &paths[path][method]
    }

    fn parameter<'a>(op: &'a Value, name: &str) -> &'a Value {
        op["parameters"]
            .as_array()
            .and_then(|params| params.iter().find(|param| param["name"] == name))
            .unwrap_or_else(|| panic!("missing parameter {name}"))
    }

    #[test]
    fn build_contains_expected_core_routes() {
        let paths = build();
        let paths = paths.as_object().expect("paths should be an object");

        let expected = [
            "/api/health",
            "/api/health/ready",
            "/api/health/live",
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
        ];

        assert_eq!(paths.len(), expected.len());

        for path in expected {
            assert!(paths.contains_key(path), "missing path {path}");
        }
    }

    #[test]
    fn sessions_and_runs_paths_document_expected_query_and_error_behavior() {
        let paths = build();

        let sessions = operation(&paths, "/api/sessions", "get");
        assert_eq!(sessions["operationId"], "listSessions");
        assert_eq!(parameter(sessions, "limit")["in"], "query");
        assert_eq!(parameter(sessions, "limit")["schema"]["default"], 50);
        assert_eq!(
            sessions["responses"]["422"]["$ref"],
            "#/components/responses/UnprocessableEntity"
        );

        let session_messages = operation(&paths, "/api/sessions/{session_key}/messages", "get");
        assert_eq!(parameter(session_messages, "session_key")["required"], true);
        assert_eq!(
            parameter(session_messages, "limit")["schema"]["default"],
            100
        );
        assert_eq!(
            session_messages["responses"]["200"]["content"]["application/json"]["schema"]["items"]
                ["$ref"],
            "#/components/schemas/MessageItem"
        );
        assert_eq!(
            session_messages["responses"]["404"]["$ref"],
            "#/components/responses/NotFound"
        );

        let runs = operation(&paths, "/api/runs", "get");
        assert_eq!(runs["operationId"], "listRuns");
        assert_eq!(
            parameter(runs, "status")["schema"]["enum"],
            json!(["running", "completed", "failed", "suspended"])
        );
        assert_eq!(parameter(runs, "limit")["schema"]["default"], 50);
    }

    #[test]
    fn workflow_routes_reference_expected_detail_and_trigger_schemas() {
        let paths = build();

        let workflow_detail = operation(&paths, "/api/workflows/{name}", "get");
        assert_eq!(workflow_detail["operationId"], "getWorkflow");
        assert_eq!(parameter(workflow_detail, "name")["required"], true);
        assert_eq!(
            workflow_detail["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/WorkflowDetail"
        );

        let workflow_trigger = operation(&paths, "/api/workflows/{name}/trigger", "post");
        assert_eq!(workflow_trigger["operationId"], "triggerWorkflow");
        assert_eq!(parameter(workflow_trigger, "name")["required"], true);
        assert_eq!(workflow_trigger["requestBody"]["required"], false);
        assert_eq!(
            workflow_trigger["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/TriggerWorkflowRequest"
        );
        assert_eq!(
            workflow_trigger["responses"]["202"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/TriggerWorkflowResponse"
        );
        assert_eq!(
            workflow_trigger["responses"]["404"]["$ref"],
            "#/components/responses/NotFound"
        );
    }

    #[test]
    fn every_core_operation_has_tag_summary_and_operation_id() {
        let paths = build();
        let paths = paths.as_object().expect("paths should be an object");

        for (path, methods) in paths {
            let methods = methods
                .as_object()
                .unwrap_or_else(|| panic!("path {path} should map to operations"));

            for (method, op) in methods {
                let tags = op["tags"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{method} {path} should declare at least one tag"));
                assert!(
                    !tags.is_empty(),
                    "{method} {path} should declare at least one tag"
                );
                assert!(
                    op["summary"].is_string(),
                    "{method} {path} should declare a summary"
                );
                assert!(
                    op["operationId"].is_string(),
                    "{method} {path} should declare an operationId"
                );
            }
        }
    }
}
