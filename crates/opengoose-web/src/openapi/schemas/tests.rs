use super::*;
use serde_json::{Value, json};

fn schema_ref(value: &Value) -> Option<&str> {
    value["content"]["application/json"]["schema"]["$ref"].as_str()
}

#[test]
fn common_responses_wrap_error_response_schema() {
    let responses = common_responses();
    let responses = responses
        .as_object()
        .expect("common responses should be an object");

    assert_eq!(responses.len(), 3);

    for (name, description) in [
        ("NotFound", "Resource not found"),
        ("InternalError", "Internal server error"),
        (
            "UnprocessableEntity",
            "Unprocessable entity — validation failed",
        ),
    ] {
        let response = responses
            .get(name)
            .unwrap_or_else(|| panic!("missing response {name}"));
        assert_eq!(response["description"], description);
        assert_eq!(
            schema_ref(response),
            Some("#/components/schemas/ErrorResponse")
        );
    }
}

#[test]
fn build_schemas_merges_domain_modules_without_dropping_entries() {
    let groups = [
        ("system", system::build()),
        ("dashboard", dashboard::build()),
        ("sessions", sessions::build()),
        ("runs", runs::build()),
        ("agents", agents::build()),
        ("teams", teams::build()),
        ("workflows", workflows::build()),
        ("events", events::build()),
        ("alerts", alerts::build()),
        ("channels", channels::build()),
        ("gateways", gateways::build()),
        ("triggers", triggers::build()),
    ];
    let merged = build_schemas();
    let merged = merged
        .as_object()
        .expect("merged schemas should be an object");

    let expected_len: usize = groups
        .iter()
        .map(|(_, group)| {
            group
                .as_object()
                .expect("schema group should be an object")
                .len()
        })
        .sum();

    assert_eq!(merged.len(), expected_len);

    for (group_name, group) in groups {
        let group = group
            .as_object()
            .unwrap_or_else(|| panic!("schema group {group_name} should be an object"));

        for key in group.keys() {
            assert!(
                merged.contains_key(key),
                "missing merged schema {key} from {group_name}"
            );
        }
    }
}

#[test]
fn system_and_session_schemas_capture_required_fields_and_enums() {
    let system = system::build();
    let sessions = sessions::build();
    let system = system
        .as_object()
        .expect("system schemas should be an object");
    let sessions = sessions
        .as_object()
        .expect("session schemas should be an object");

    assert_eq!(
        system["HealthResponse"]["required"],
        json!(["status", "version", "checked_at", "components"])
    );
    assert_eq!(
        system["HealthStatus"]["enum"],
        json!(["healthy", "degraded", "unavailable"])
    );
    assert_eq!(
        system["HealthComponents"]["required"],
        json!(["database", "cron_scheduler", "alert_dispatcher", "gateways"])
    );
    assert_eq!(
        sessions["SessionItem"]["properties"]["active_team"]["nullable"],
        true
    );
    assert_eq!(
        sessions["MessageItem"]["properties"]["role"]["enum"],
        json!(["user", "assistant", "system"])
    );
}

#[test]
fn run_agent_and_team_schemas_capture_expected_fields() {
    let runs = runs::build();
    let agents = agents::build();
    let teams = teams::build();
    let runs = runs.as_object().expect("run schemas should be an object");
    let agents = agents
        .as_object()
        .expect("agent schemas should be an object");
    let teams = teams.as_object().expect("team schemas should be an object");

    assert_eq!(
        runs["RunItem"]["properties"]["created_at"]["format"],
        "date-time"
    );
    assert_eq!(agents["AgentItem"]["required"], json!(["name"]));
    assert_eq!(
        teams["TeamItem"]["properties"]["workflow"]["enum"],
        json!(["chain", "fan-out", "router"])
    );
}

#[test]
fn workflow_and_event_schemas_define_expected_nested_refs() {
    let workflows = workflows::build();
    let events = events::build();
    let workflows = workflows
        .as_object()
        .expect("workflow schemas should be an object");
    let events = events
        .as_object()
        .expect("event schemas should be an object");

    assert_eq!(
        workflows["WorkflowAutomation"]["properties"]["kind"]["enum"],
        json!(["schedule", "trigger"])
    );
    assert_eq!(
        workflows["WorkflowDetail"]["properties"]["steps"]["items"]["$ref"],
        "#/components/schemas/WorkflowStep"
    );
    assert_eq!(
        workflows["TriggerWorkflowResponse"]["required"],
        json!(["workflow", "accepted", "input"])
    );
    assert_eq!(
        events["EventHistoryPageResponse"]["properties"]["items"]["items"]["$ref"],
        "#/components/schemas/EventHistoryResponse"
    );
}

#[test]
fn ops_domain_schemas_define_expected_enums_and_nested_refs() {
    let alerts = alerts::build();
    let channels = channels::build();
    let gateways = gateways::build();
    let triggers = triggers::build();
    let alerts = alerts
        .as_object()
        .expect("alert schemas should be an object");
    let channels = channels
        .as_object()
        .expect("channel schemas should be an object");
    let gateways = gateways
        .as_object()
        .expect("gateway schemas should be an object");
    let triggers = triggers
        .as_object()
        .expect("trigger schemas should be an object");

    assert_eq!(
        alerts["AlertRuleResponse"]["properties"]["metric"]["enum"],
        json!(["queue_backlog", "failed_runs", "error_rate"])
    );
    assert_eq!(
        channels["ChannelMetricsResponse"]["properties"]["platforms"]["additionalProperties"]["$ref"],
        "#/components/schemas/ChannelMetricsSnapshot"
    );
    assert_eq!(
        gateways["GatewayListResponse"]["properties"]["gateways"]["items"]["$ref"],
        "#/components/schemas/GatewaySummary"
    );
    assert_eq!(
        triggers["TriggerTestResponse"]["properties"]["fired"]["type"],
        "boolean"
    );
}

#[test]
fn every_required_field_is_declared_in_schema_properties() {
    let schemas = build_schemas();
    let schemas = schemas
        .as_object()
        .expect("merged schemas should be an object");

    for (name, schema) in schemas {
        if schema["type"] != "object" {
            continue;
        }
        let properties = schema["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("schema {name} should declare properties"));

        if let Some(required) = schema["required"].as_array() {
            for field in required {
                let field = field
                    .as_str()
                    .unwrap_or_else(|| panic!("schema {name} required fields must be strings"));
                assert!(
                    properties.contains_key(field),
                    "schema {name} is missing property {field}"
                );
            }
        }
    }
}
