use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
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
        }
    })
}
