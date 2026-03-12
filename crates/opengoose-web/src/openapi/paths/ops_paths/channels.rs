use serde_json::{Value, json};

pub(super) fn build() -> Value {
    json!({
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
        }
    })
}
