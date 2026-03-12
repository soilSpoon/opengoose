use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
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
        },
        "/api/events/history": {
            "get": {
                "tags": ["events"],
                "summary": "List persisted event history",
                "operationId": "listEventHistory",
                "parameters": [
                    {
                        "name": "limit",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "integer", "default": 100, "minimum": 1, "maximum": 1000 }
                    },
                    {
                        "name": "offset",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "integer", "default": 0, "minimum": 0 }
                    },
                    {
                        "name": "gateway",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "string" }
                    },
                    {
                        "name": "kind",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "string" }
                    },
                    {
                        "name": "session_key",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "string" }
                    },
                    {
                        "name": "since",
                        "in": "query",
                        "required": false,
                        "description": "Relative duration like `24h` or an absolute RFC3339 / SQLite timestamp",
                        "schema": { "type": "string" }
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Paginated event history",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/EventHistoryPageResponse" }
                            }
                        }
                    },
                    "422": { "$ref": "#/components/responses/UnprocessableEntity" },
                    "500": { "$ref": "#/components/responses/InternalError" }
                }
            }
        }
    })
}
