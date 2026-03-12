/// Resource paths: agents, teams, and workflows.
use serde_json::{Value, json};

pub(in crate::openapi) fn build() -> Value {
    json!({
        "/api/agents": {
            "get": {
                "tags": ["agents"],
                "summary": "List configured agents",
                "operationId": "listAgents",
                "responses": {
                    "200": {
                        "description": "List of configured agents",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": "#/components/schemas/AgentItem" }
                                }
                            }
                        }
                    }
                }
            }
        },
        "/api/teams": {
            "get": {
                "tags": ["teams"],
                "summary": "List teams",
                "operationId": "listTeams",
                "responses": {
                    "200": {
                        "description": "List of configured teams",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": "#/components/schemas/TeamItem" }
                                }
                            }
                        }
                    }
                }
            }
        },
        "/api/workflows": {
            "get": {
                "tags": ["workflows"],
                "summary": "List workflows",
                "operationId": "listWorkflows",
                "responses": {
                    "200": {
                        "description": "Available workflow definitions",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": "#/components/schemas/WorkflowItem" }
                                }
                            }
                        }
                    }
                }
            }
        },
        "/api/workflows/{name}": {
            "get": {
                "tags": ["workflows"],
                "summary": "Get workflow details",
                "operationId": "getWorkflow",
                "parameters": [
                    {
                        "name": "name",
                        "in": "path",
                        "required": true,
                        "schema": { "type": "string" }
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Workflow definition with parameters",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/WorkflowDetail" }
                            }
                        }
                    },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            }
        },
        "/api/workflows/{name}/trigger": {
            "post": {
                "tags": ["workflows"],
                "summary": "Trigger a workflow",
                "operationId": "triggerWorkflow",
                "parameters": [
                    {
                        "name": "name",
                        "in": "path",
                        "required": true,
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
