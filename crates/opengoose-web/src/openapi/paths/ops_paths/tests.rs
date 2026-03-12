use super::*;
use serde_json::{Value, json};

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
