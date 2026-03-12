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
fn build_contains_expected_core_routes() {
    let paths = super::build();
    let paths = paths.as_object().expect("paths should be an object");

    let expected = [
        "/api/health",
        "/api/health/ready",
        "/api/health/live",
        "/api/metrics",
        "/api/dashboard",
        "/api/sessions",
        "/api/sessions/{session_key}/messages",
        "/api/runs",
        "/api/agents",
        "/api/teams",
        "/api/workflows",
        "/api/workflows/{name}",
        "/api/workflows/{name}/trigger",
    ];

    assert_eq!(paths.len(), expected.len());

    for path in expected {
        assert!(paths.contains_key(path), "missing path {path}");
    }
}

#[test]
fn sessions_and_runs_paths_document_expected_query_and_error_behavior() {
    let paths = super::build();

    let sessions = operation(&paths, "/api/sessions", "get");
    assert_eq!(sessions["operationId"], "listSessions");
    assert_eq!(parameter(sessions, "limit")["in"], "query");
    assert_eq!(parameter(sessions, "limit")["schema"]["default"], 50);
    assert_eq!(
        sessions["responses"]["422"]["$ref"],
        "#/components/responses/UnprocessableEntity"
    );

    let session_messages = operation(&paths, "/api/sessions/{session_key}/messages", "get");
    assert_eq!(parameter(session_messages, "session_key")["required"], true);
    assert_eq!(
        parameter(session_messages, "limit")["schema"]["default"],
        100
    );
    assert_eq!(
        session_messages["responses"]["200"]["content"]["application/json"]["schema"]["items"]["$ref"],
        "#/components/schemas/MessageItem"
    );
    assert_eq!(
        session_messages["responses"]["404"]["$ref"],
        "#/components/responses/NotFound"
    );

    let runs = operation(&paths, "/api/runs", "get");
    assert_eq!(runs["operationId"], "listRuns");
    assert_eq!(
        parameter(runs, "status")["schema"]["enum"],
        json!(["running", "completed", "failed", "suspended"])
    );
    assert_eq!(parameter(runs, "limit")["schema"]["default"], 50);
}

#[test]
fn workflow_routes_reference_expected_detail_and_trigger_schemas() {
    let paths = super::build();

    let workflow_detail = operation(&paths, "/api/workflows/{name}", "get");
    assert_eq!(workflow_detail["operationId"], "getWorkflow");
    assert_eq!(parameter(workflow_detail, "name")["required"], true);
    assert_eq!(
        workflow_detail["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/WorkflowDetail"
    );

    let workflow_trigger = operation(&paths, "/api/workflows/{name}/trigger", "post");
    assert_eq!(workflow_trigger["operationId"], "triggerWorkflow");
    assert_eq!(parameter(workflow_trigger, "name")["required"], true);
    assert_eq!(workflow_trigger["requestBody"]["required"], false);
    assert_eq!(
        workflow_trigger["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/TriggerWorkflowRequest"
    );
    assert_eq!(
        workflow_trigger["responses"]["202"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/TriggerWorkflowResponse"
    );
    assert_eq!(
        workflow_trigger["responses"]["404"]["$ref"],
        "#/components/responses/NotFound"
    );
}

#[test]
fn every_core_operation_has_tag_summary_and_operation_id() {
    let paths = super::build();
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
