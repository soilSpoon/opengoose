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
                "parameters": [
                    {
                        "name": "limit",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "integer", "default": 50, "minimum": 1, "maximum": 1000 }
                    },
                    {
                        "name": "offset",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "integer", "default": 0, "minimum": 0 }
                    },
                    {
                        "name": "rule",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "string" },
                        "description": "Optional exact alert rule name to filter history entries"
                    },
                    {
                        "name": "since",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "string" },
                        "description": "Optional cutoff timestamp for events (supports `24h`, `7d`, RFC3339, or `YYYY-MM-DD HH:MM:SS`)"
                    }
                ],
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
                    "422": { "$ref": "#/components/responses/UnprocessableEntity" },
                    "500": { "$ref": "#/components/responses/InternalError" }
                }
            }
        },
        "/api/alerts/test": {
            "post": {
                "tags": ["alerts"],
                "summary": "Evaluate all alert rules immediately",
                "operationId": "testAlerts",
                "parameters": [
                    {
                        "name": "rule",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "string" },
                        "description": "Optional rule name to evaluate"
                    },
                    {
                        "name": "dry_run",
                        "in": "query",
                        "required": false,
                        "schema": { "type": "boolean", "default": false },
                        "description": "Evaluate without recording matches"
                    }
                ],
                "responses": {
                    "200": {
                        "description": "Evaluation summary",
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/TestAlertsResponse" }
                            }
                        }
                    },
                    "404": { "$ref": "#/components/responses/NotFound" },
                    "422": { "$ref": "#/components/responses/UnprocessableEntity" },
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
    fn build_contains_expected_ops_routes() {
        let paths = build();
        let paths = paths.as_object().expect("paths should be an object");

        let expected = [
            "/api/alerts",
            "/api/alerts/history",
            "/api/alerts/test",
            "/api/alerts/{name}",
            "/api/triggers",
            "/api/triggers/{name}",
            "/api/triggers/{name}/enabled",
            "/api/triggers/{name}/test",
            "/api/channel-metrics",
            "/api/gateways",
            "/api/gateways/{platform}/status",
            "/api/webhooks/{path}",
            "/api/events",
            "/api/events/history",
        ];

        assert_eq!(paths.len(), expected.len());

        for path in expected {
            assert!(paths.contains_key(path), "missing path {path}");
        }
    }

    #[test]
    fn alert_routes_reference_expected_schemas_and_validation_responses() {
        let paths = build();

        let list_alerts = operation(&paths, "/api/alerts", "get");
        assert_eq!(list_alerts["operationId"], "listAlerts");
        assert_eq!(
            list_alerts["responses"]["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/AlertRuleResponse"
        );

        let create_alert = operation(&paths, "/api/alerts", "post");
        assert_eq!(create_alert["operationId"], "createAlert");
        assert_eq!(create_alert["requestBody"]["required"], true);
        assert_eq!(
            create_alert["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/CreateAlertRequest"
        );
        assert_eq!(
            create_alert["responses"]["201"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/AlertRuleResponse"
        );
        assert_eq!(
            create_alert["responses"]["422"]["$ref"],
            "#/components/responses/UnprocessableEntity"
        );

        let delete_alert = operation(&paths, "/api/alerts/{name}", "delete");
        assert_eq!(parameter(delete_alert, "name")["required"], true);
        assert_eq!(
            delete_alert["responses"]["404"]["$ref"],
            "#/components/responses/NotFound"
        );
    }

    #[test]
    fn trigger_routes_require_path_parameters_and_correct_payloads() {
        let paths = build();

        let get_trigger = operation(&paths, "/api/triggers/{name}", "get");
        assert_eq!(parameter(get_trigger, "name")["required"], true);
        assert_eq!(
            get_trigger["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/TriggerItem"
        );

        let update_trigger = operation(&paths, "/api/triggers/{name}", "put");
        assert_eq!(update_trigger["requestBody"]["required"], true);
        assert_eq!(
            update_trigger["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/UpdateTriggerRequest"
        );

        let set_enabled = operation(&paths, "/api/triggers/{name}/enabled", "patch");
        assert_eq!(parameter(set_enabled, "name")["required"], true);
        assert_eq!(set_enabled["requestBody"]["required"], true);
        assert_eq!(
            set_enabled["requestBody"]["content"]["application/json"]["schema"]["required"],
            json!(["enabled"])
        );
        assert_eq!(
            set_enabled["responses"]["200"]["description"],
            "Updated successfully"
        );

        let test_trigger = operation(&paths, "/api/triggers/{name}/test", "post");
        assert_eq!(parameter(test_trigger, "name")["required"], true);
        assert_eq!(
            test_trigger["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/TriggerTestResponse"
        );
    }

    #[test]
    fn gateway_webhook_and_event_routes_cover_special_protocols() {
        let paths = build();

        let gateway_status = operation(&paths, "/api/gateways/{platform}/status", "get");
        assert_eq!(parameter(gateway_status, "platform")["required"], true);
        assert_eq!(
            gateway_status["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/GatewayStatusResponse"
        );

        let webhook = operation(&paths, "/api/webhooks/{path}", "post");
        assert_eq!(parameter(webhook, "path")["required"], true);
        assert_eq!(webhook["requestBody"]["required"], false);
        assert_eq!(
            webhook["requestBody"]["content"]["application/json"]["schema"]["type"],
            "object"
        );
        assert_eq!(
            webhook["responses"]["404"]["description"],
            "No matching trigger found for this path"
        );

        let events = operation(&paths, "/api/events", "get");
        assert_eq!(events["operationId"], "streamEvents");
        assert_eq!(
            events["responses"]["200"]["content"]["text/event-stream"]["schema"]["type"],
            "string"
        );
        assert!(
            events["description"]
                .as_str()
                .expect("events endpoint should describe the stream")
                .contains("session, run, queue")
        );

        let history = operation(&paths, "/api/events/history", "get");
        assert_eq!(history["operationId"], "listEventHistory");
        assert_eq!(parameter(history, "limit")["schema"]["default"], 100);
        assert_eq!(parameter(history, "offset")["schema"]["default"], 0);
        assert_eq!(
            history["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/EventHistoryPageResponse"
        );
    }

    #[test]
    fn every_ops_operation_has_tag_summary_and_operation_id() {
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
