/// Session and run paths.
use serde_json::{Value, json};

pub(in crate::openapi) fn build() -> Value {
    json!({
        "/api/dashboard": {
            "get": {
                "tags": ["dashboard"],
                "summary": "Dashboard overview",
                "operationId": "getDashboard",
                "responses": {
                    "200": {
                        "description": "Key metrics and recent activity",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/DashboardResponse" }
                            }
                        }
                    }
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
                        "schema": { "type": "integer", "default": 50 }
                    },
                    {
                        "name": "offset",
                        "in": "query",
                        "schema": { "type": "integer", "default": 0 }
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Paginated list of sessions",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": "#/components/schemas/SessionSummary" }
                                }
                            }
                        }
                    },
                    "422": { "$ref": "#/components/responses/UnprocessableEntity" }
                }
            }
        },
        "/api/sessions/{session_key}/messages": {
            "get": {
                "tags": ["sessions"],
                "summary": "List messages in a session",
                "operationId": "listSessionMessages",
                "parameters": [
                    {
                        "name": "session_key",
                        "in": "path",
                        "required": true,
                        "schema": { "type": "string" }
                    },
                    {
                        "name": "limit",
                        "in": "query",
                        "schema": { "type": "integer", "default": 100 }
                    },
                    {
                        "name": "offset",
                        "in": "query",
                        "schema": { "type": "integer", "default": 0 }
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Paginated list of messages for the session",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": "#/components/schemas/MessageItem" }
                                }
                            }
                        }
                    },
                    "404": { "$ref": "#/components/responses/NotFound" }
                }
            }
        },
        "/api/runs": {
            "get": {
                "tags": ["runs"],
                "summary": "List runs",
                "operationId": "listRuns",
                "parameters": [
                    {
                        "name": "status",
                        "in": "query",
                        "schema": {
                            "type": "string",
                            "enum": ["running", "completed", "failed", "suspended"]
                        }
                    },
                    {
                        "name": "limit",
                        "in": "query",
                        "schema": { "type": "integer", "default": 50 }
                    },
                    {
                        "name": "offset",
                        "in": "query",
                        "schema": { "type": "integer", "default": 0 }
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Paginated list of runs",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "array",
                                    "items": { "$ref": "#/components/schemas/RunSummary" }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}
