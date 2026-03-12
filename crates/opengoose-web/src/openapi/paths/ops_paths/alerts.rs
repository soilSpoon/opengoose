use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
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
        }
    })
}
