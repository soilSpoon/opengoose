use serde_json::{Value, json};

/// Operations API paths: alerts, triggers, channels, gateways, webhooks, events.
pub(in crate::openapi) fn build() -> Value {
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
    })
}
