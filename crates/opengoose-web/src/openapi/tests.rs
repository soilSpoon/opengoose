use super::*;

// --- Spec structure tests ---

#[test]
fn spec_is_valid_json() {
    let spec = build_spec();
    let json = serde_json::to_string(&spec).expect("spec should serialize to JSON");
    assert!(json.contains("openapi"));
    assert!(json.contains("OpenGoose Web API"));
}

#[test]
fn spec_has_correct_openapi_version() {
    let spec = build_spec();
    assert_eq!(spec["openapi"], "3.0.3");
}

#[test]
fn spec_info_contains_required_fields() {
    let spec = build_spec();
    let info = &spec["info"];
    assert_eq!(info["title"], "OpenGoose Web API");
    assert!(info["version"].is_string(), "version should be a string");
    assert!(info["description"].is_string(), "description should be present");
    assert_eq!(info["license"]["name"], "MIT");
    assert!(info["contact"]["url"].as_str().unwrap().contains("github.com"));
}

// --- Path completeness tests ---

#[test]
fn spec_contains_all_api_paths() {
    let spec = build_spec();
    let paths = spec["paths"].as_object().expect("paths should be an object");

    let expected = [
        "/api/health",
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
    ];

    for path in &expected {
        assert!(paths.contains_key(*path), "missing path: {path}");
    }
    assert_eq!(paths.len(), expected.len(), "unexpected extra paths in spec");
}

// --- Schema completeness tests ---

#[test]
fn spec_contains_schema_components() {
    let spec = build_spec();
    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("schemas should be an object");

    let expected_schemas = [
        "ErrorResponse",
        "HealthResponse",
        "DashboardStats",
        "SessionItem",
        "MessageItem",
        "RunItem",
        "AgentItem",
        "TeamItem",
        "WorkflowItem",
        "WorkflowDetail",
        "AlertRuleResponse",
        "CreateAlertRequest",
        "TriggerItem",
        "GatewaySummary",
        "GatewayListResponse",
    ];

    for schema in &expected_schemas {
        assert!(schemas.contains_key(*schema), "missing schema: {schema}");
    }
}

#[test]
fn spec_contains_all_response_components() {
    let spec = build_spec();
    let responses = spec["components"]["responses"]
        .as_object()
        .expect("responses should be an object");

    for name in &["NotFound", "InternalError", "UnprocessableEntity"] {
        assert!(responses.contains_key(*name), "missing response: {name}");
    }
}

// --- Tag tests ---

#[test]
fn spec_contains_all_tags() {
    let spec = build_spec();
    let tags = spec["tags"].as_array().expect("tags should be an array");
    let tag_names: Vec<&str> = tags
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    let expected = [
        "system", "dashboard", "sessions", "runs", "agents", "teams",
        "workflows", "alerts", "triggers", "channels", "gateways",
        "webhooks", "events",
    ];
    for name in &expected {
        assert!(tag_names.contains(name), "missing tag: {name}");
    }
}

// --- Endpoint documentation accuracy ---

#[test]
fn every_path_operation_has_operation_id() {
    let spec = build_spec();
    let paths = spec["paths"].as_object().unwrap();
    for (path, methods) in paths {
        let methods = methods.as_object().unwrap();
        for (method, op) in methods {
            assert!(
                op["operationId"].is_string(),
                "{method} {path} missing operationId"
            );
        }
    }
}

#[test]
fn every_path_operation_has_tags() {
    let spec = build_spec();
    let paths = spec["paths"].as_object().unwrap();
    for (path, methods) in paths {
        let methods = methods.as_object().unwrap();
        for (method, op) in methods {
            let tags = op["tags"].as_array();
            assert!(
                tags.is_some() && !tags.unwrap().is_empty(),
                "{method} {path} missing tags"
            );
        }
    }
}

#[test]
fn operation_ids_are_unique() {
    let spec = build_spec();
    let paths = spec["paths"].as_object().unwrap();
    let mut ids = std::collections::HashSet::new();
    for (_path, methods) in paths {
        let methods = methods.as_object().unwrap();
        for (_method, op) in methods {
            if let Some(id) = op["operationId"].as_str() {
                assert!(ids.insert(id.to_string()), "duplicate operationId: {id}");
            }
        }
    }
}

#[test]
fn path_parameters_are_required() {
    let spec = build_spec();
    let paths = spec["paths"].as_object().unwrap();
    for (path, methods) in paths {
        let methods = methods.as_object().unwrap();
        for (_method, op) in methods {
            if let Some(params) = op["parameters"].as_array() {
                for param in params {
                    if param["in"] == "path" {
                        assert_eq!(
                            param["required"], true,
                            "path param '{}' in {path} must be required",
                            param["name"]
                        );
                    }
                }
            }
        }
    }
}

// --- Response schema validation ---

#[test]
fn schema_refs_point_to_existing_schemas() {
    let spec = build_spec();
    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("schemas should exist");
    let responses = spec["components"]["responses"]
        .as_object()
        .expect("responses should exist");

    let json_str = serde_json::to_string(&spec).unwrap();
    // Find all $ref values by string scanning
    let schema_prefix = "#/components/schemas/";
    let response_prefix = "#/components/responses/";
    for line in json_str.split('"') {
        if let Some(name) = line.strip_prefix(schema_prefix) {
            assert!(schemas.contains_key(name), "dangling $ref: schemas/{name}");
        } else if let Some(name) = line.strip_prefix(response_prefix) {
            assert!(responses.contains_key(name), "dangling $ref: responses/{name}");
        }
    }
}

#[test]
fn all_schemas_have_type_field() {
    let spec = build_spec();
    let schemas = spec["components"]["schemas"].as_object().unwrap();
    for (name, schema) in schemas {
        assert!(
            schema["type"].is_string(),
            "schema {name} missing 'type' field"
        );
    }
}

#[test]
fn required_fields_exist_in_properties() {
    let spec = build_spec();
    let schemas = spec["components"]["schemas"].as_object().unwrap();
    for (name, schema) in schemas {
        if let Some(required) = schema["required"].as_array() {
            let props = schema["properties"].as_object();
            assert!(props.is_some(), "schema {name} has required but no properties");
            let props = props.unwrap();
            for req_field in required {
                let field_name = req_field.as_str().unwrap();
                assert!(
                    props.contains_key(field_name),
                    "schema {name}: required field '{field_name}' not in properties"
                );
            }
        }
    }
}

// --- Handler tests ---

#[tokio::test]
async fn serve_openapi_json_returns_content_type_json() {
    use axum::response::IntoResponse;
    let response = serve_openapi_json().await.into_response();
    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type should be set")
        .to_str()
        .expect("content-type should be valid string");
    assert!(content_type.contains("application/json"));
}

#[tokio::test]
async fn serve_openapi_json_body_is_parseable() {
    use axum::body::to_bytes;
    use axum::response::IntoResponse;
    let response = serve_openapi_json().await.into_response();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("body should be valid JSON");
    assert_eq!(parsed["openapi"], "3.0.3");
}

#[tokio::test]
async fn serve_swagger_ui_returns_html_with_swagger_ui() {
    let html = serve_swagger_ui().await;
    assert!(html.0.contains("swagger-ui"));
    assert!(html.0.contains("/api/openapi.json"));
}

// --- Path merge correctness ---

#[test]
fn core_and_ops_paths_do_not_overlap() {
    let core = paths::core_paths::build();
    let ops = paths::ops_paths::build();
    let core_keys: std::collections::HashSet<&str> = core
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    let ops_keys: std::collections::HashSet<&str> = ops
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    let overlap: Vec<&&str> = core_keys.intersection(&ops_keys).collect();
    assert!(overlap.is_empty(), "overlapping paths: {overlap:?}");
}
