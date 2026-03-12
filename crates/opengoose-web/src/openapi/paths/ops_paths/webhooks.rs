use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
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
        }
    })
}
