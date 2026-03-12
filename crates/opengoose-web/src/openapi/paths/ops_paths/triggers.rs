use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
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
        }
    })
}
