use std::future::Future;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::Html;
use opengoose_persistence::{
    Database, OrchestrationStore, ScheduleStore, SessionStore, TriggerStore,
};
use opengoose_teams::remote::{RemoteAgentRegistry, RemoteConfig};
use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore};
use opengoose_types::{ChannelMetricsStore, EventBus, Platform, SessionKey};
use tower::ServiceExt;

use super::catalog::{
    AgentQuery, RunQuery, ScheduleActionForm, ScheduleQuery, SessionQuery, TeamSaveForm,
    TriggerQuery, WorkflowQuery, agents, queue, runs, schedule_action, schedules, sessions,
    team_save, triggers, workflows,
};
use super::dashboard::dashboard;
use super::remote_agents::{remote_agents, websocket_url};
use super::router;
use crate::server::PageState;
use crate::test_support::with_temp_home;

fn save_team(name: &str) {
    TeamStore::new()
        .expect("team store should open")
        .save(
            &TeamDefinition {
                version: "1.0.0".into(),
                title: name.into(),
                description: Some(format!("{name} team")),
                workflow: OrchestrationPattern::Chain,
                agents: vec![TeamAgent {
                    profile: "tester".into(),
                    role: Some("validate setup".into()),
                }],
                router: None,
                fan_out: None,
            },
            true,
        )
        .expect("team should save");
}

fn page_state(db: Arc<Database>) -> PageState {
    PageState {
        db,
        remote_registry: RemoteAgentRegistry::new(RemoteConfig::default()),
        channel_metrics: ChannelMetricsStore::new(),
        event_bus: EventBus::new(256),
    }
}

fn run_async(test: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build")
        .block_on(test);
}

fn save_session(db: Arc<Database>, key: &SessionKey, active_team: Option<&str>) {
    let store = SessionStore::new(db);
    store
        .append_user_message(key, "Need a reviewer on this run.", Some("tester"))
        .expect("session should seed");
    if let Some(team) = active_team {
        store
            .set_active_team(key, Some(team))
            .expect("active team should seed");
    }
}

fn save_run(db: Arc<Database>, run_id: &str) {
    OrchestrationStore::new(db)
        .create_run(
            run_id,
            "discord:ns:ops:chan-1",
            "ops",
            "chain",
            "Review the latest deploy.",
            3,
        )
        .expect("run should seed");
}

async fn read_body(response: axum::response::Response) -> String {
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body should be readable");
    String::from_utf8(body.to_vec()).expect("response body should be utf-8")
}

#[test]
fn page_router_get_routes_return_expected_statuses() {
    with_temp_home("opengoose-routes-pages-home", || {
        run_async(async {
            let app = router(page_state(Arc::new(
                Database::open_in_memory().expect("db should open"),
            )));

            for path in [
                "/",
                "/dashboard/events",
                "/sessions",
                "/runs",
                "/agents",
                "/remote-agents",
                "/remote-agents/events",
                "/workflows",
                "/schedules",
                "/triggers",
                "/teams",
                "/queue",
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(Method::GET)
                            .uri(path)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .expect("request should be handled");

                assert_eq!(
                    response.status(),
                    StatusCode::OK,
                    "path `{path}` should render"
                );
            }
        });
    });
}

#[test]
fn page_router_post_routes_return_expected_statuses() {
    with_temp_home("opengoose-routes-pages-home", || {
        run_async(async {
            let app = router(page_state(Arc::new(
                Database::open_in_memory().expect("db should open"),
            )));

            let schedule_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/schedules")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("intent=unsupported"))
                        .unwrap(),
                )
                .await
                .expect("schedule request should be handled");
            assert_eq!(schedule_response.status(), StatusCode::BAD_REQUEST);

            let team_response = app
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/teams")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("original_name=broken&yaml=title%3A+broken"))
                        .unwrap(),
                )
                .await
                .expect("team request should be handled");
            assert_eq!(team_response.status(), StatusCode::OK);
        });
    });
}

#[tokio::test]
async fn dashboard_handler_renders_mock_preview() {
    let Html(html) = dashboard(State(page_state(Arc::new(
        Database::open_in_memory().expect("db should open"),
    ))))
    .await
    .expect("handler should render");

    assert!(html.contains("Mock preview"));
    assert!(html.contains("No runtime data yet"));
}

#[tokio::test]
async fn sessions_handler_invalid_selection_falls_back_to_live_session() {
    let db = Arc::new(Database::open_in_memory().expect("db should open"));
    let session_key = SessionKey::new(Platform::Discord, "ops", "chan-1");
    save_session(db.clone(), &session_key, Some("reviewers"));

    let Html(html) = sessions(
        State(page_state(db)),
        Query(SessionQuery {
            session: Some("discord:ns:missing:session".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("Live runtime"));
    assert!(html.contains("Session chan-1"));
    assert!(html.contains(&session_key.to_stable_id()));
}

#[tokio::test]
async fn runs_handler_invalid_selection_falls_back_to_live_run() {
    let db = Arc::new(Database::open_in_memory().expect("db should open"));
    save_run(db.clone(), "run-live-1");

    let Html(html) = runs(
        State(page_state(db)),
        Query(RunQuery {
            run: Some("missing-run".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("Live runtime"));
    assert!(html.contains("Run run-live-1"));
    assert!(html.contains("ops / chain"));
}

#[test]
fn agents_handler_renders_bundled_defaults_for_unknown_selection() {
    with_temp_home("opengoose-routes-pages-home", || {
        run_async(async {
            let Html(html) = agents(Query(AgentQuery {
                agent: Some("missing-agent".into()),
            }))
            .await
            .expect("handler should render");

            assert!(html.contains("Bundled defaults"));
            assert!(html.contains("aria-current=\"page\""));
        });
    });
}

#[test]
fn workflows_handler_renders_bundled_defaults_for_unknown_selection() {
    with_temp_home("opengoose-routes-pages-home", || {
        run_async(async {
            let Html(html) = workflows(
                State(page_state(Arc::new(
                    Database::open_in_memory().expect("db should open"),
                ))),
                Query(WorkflowQuery {
                    workflow: Some("missing-workflow".into()),
                }),
            )
            .await
            .expect("handler should render");

            assert!(html.contains("Bundled defaults"));
            assert!(html.contains("Workflow detail"));
            assert!(html.contains("aria-current=\"page\""));
        });
    });
}

#[tokio::test]
async fn triggers_handler_invalid_selection_falls_back_to_existing_trigger() {
    let db = Arc::new(Database::open_in_memory().expect("db should open"));
    TriggerStore::new(db.clone())
        .create("incoming", "webhook_received", "{}", "ops", "")
        .expect("trigger should seed");

    let Html(html) = triggers(
        State(page_state(db)),
        Query(TriggerQuery {
            trigger: Some("missing-trigger".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("1 trigger(s)"));
    assert!(html.contains("incoming"));
    assert!(html.contains("webhook_received"));
}

#[tokio::test]
async fn queue_handler_invalid_selection_falls_back_to_live_run() {
    let db = Arc::new(Database::open_in_memory().expect("db should open"));
    save_run(db.clone(), "run-queue-1");

    let Html(html) = queue(
        State(page_state(db)),
        Query(RunQuery {
            run: Some("missing-run".into()),
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("Live runtime"));
    assert!(html.contains("Queue run-queue-1"));
    assert!(html.contains("No queue traffic has been recorded for this run yet."));
}

#[test]
fn team_save_invalid_yaml_renders_editor_error_notice() {
    with_temp_home("opengoose-routes-pages-home", || {
        run_async(async {
            let Html(html) = team_save(Form(TeamSaveForm {
                original_name: "broken-team".into(),
                yaml: "title: broken-team".into(),
            }))
            .await
            .expect("handler should render");

            assert!(html.contains("Fix the YAML validation error and try again."));
            assert!(html.contains("Editor draft"));
        });
    });
}

#[test]
fn schedule_action_missing_team_renders_validation_notice() {
    with_temp_home("opengoose-routes-pages-home", || {
        run_async(async {
            let Html(html) = schedule_action(
                State(page_state(Arc::new(
                    Database::open_in_memory().expect("db should open"),
                ))),
                Form(ScheduleActionForm {
                    intent: "save".into(),
                    original_name: None,
                    name: Some("nightly-ops".into()),
                    cron_expression: Some("0 0 * * * *".into()),
                    team_name: Some("missing-team".into()),
                    input: Some(String::new()),
                    enabled: Some("yes".into()),
                    confirm_delete: None,
                }),
            )
            .await
            .expect("handler should render");

            assert!(html.contains("The selected team is not installed."));
            assert!(html.contains("nightly-ops"));
        });
    });
}

#[test]
fn schedules_handler_renders_existing_schedule() {
    with_temp_home("opengoose-routes-pages-home", || {
        save_team("ops");
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        ScheduleStore::new(db.clone())
            .create(
                "nightly-ops",
                "0 0 * * * *",
                "ops",
                "",
                Some("2026-03-11 00:00:00"),
            )
            .expect("schedule should seed");

        run_async(async {
            let Html(html) = schedules(
                State(page_state(db)),
                Query(ScheduleQuery {
                    schedule: Some("nightly-ops".into()),
                }),
            )
            .await
            .expect("handler should render");

            assert!(html.contains("nightly-ops"));
            assert!(html.contains("Recent matching runs"));
        });
    });
}

#[test]
fn schedule_action_creates_schedule_from_form_post() {
    with_temp_home("opengoose-routes-pages-home", || {
        save_team("ops");
        let db = Arc::new(Database::open_in_memory().expect("db should open"));
        run_async(async {
            let Html(html) = schedule_action(
                State(page_state(db.clone())),
                Form(ScheduleActionForm {
                    intent: "save".into(),
                    original_name: None,
                    name: Some("nightly-ops".into()),
                    cron_expression: Some("0 0 * * * *".into()),
                    team_name: Some("ops".into()),
                    input: Some(String::new()),
                    enabled: Some("yes".into()),
                    confirm_delete: None,
                }),
            )
            .await
            .expect("save action should render");

            assert!(html.contains("Schedule created."));
            assert!(
                ScheduleStore::new(db)
                    .get_by_name("nightly-ops")
                    .expect("lookup should succeed")
                    .is_some()
            );
        });
    });
}

#[test]
fn schedule_action_unsupported_intent_returns_bad_request() {
    with_temp_home("opengoose-routes-pages-home", || {
        run_async(async {
            let response = router(page_state(Arc::new(
                Database::open_in_memory().expect("db should open"),
            )))
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/schedules")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("intent=unsupported"))
                    .unwrap(),
            )
            .await
            .expect("request should be handled");

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let html = read_body(response).await;
            assert!(html.contains("Unsupported schedule action."));
        });
    });
}

#[tokio::test]
async fn remote_agents_handler_renders_empty_registry() {
    let mut headers = HeaderMap::new();
    headers.insert("host", "opengoose.test".parse().expect("host header"));

    let Html(html) = remote_agents(
        State(page_state(Arc::new(
            Database::open_in_memory().expect("db should open"),
        ))),
        headers,
    )
    .await
    .expect("handler should render");

    assert!(html.contains("No remote agents are connected right now."));
    assert!(html.contains("ws://opengoose.test/api/agents/connect"));
    assert!(html.contains(
        "data-init=\"@get('/remote-agents/events', { openWhenHidden: true, retry: 'always' })\""
    ));
}

#[tokio::test]
async fn remote_agents_handler_renders_registered_agents() {
    let state = page_state(Arc::new(
        Database::open_in_memory().expect("db should open"),
    ));
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    state
        .remote_registry
        .register(
            "remote-a".into(),
            vec!["execute".into(), "relay".into()],
            "ws://remote-a:9000".into(),
            tx,
        )
        .await
        .expect("agent should register");

    let mut headers = HeaderMap::new();
    headers.insert("host", "dashboard.local".parse().expect("host header"));

    let Html(html) = remote_agents(State(state), headers)
        .await
        .expect("handler should render");

    assert!(html.contains("remote-a"));
    assert!(html.contains("execute"));
    assert!(html.contains("ws://remote-a:9000"));
    assert!(html.contains("/remote-agents/remote-a/disconnect"));
    assert!(html.contains("Disconnect"));
}

#[test]
fn websocket_url_prefers_forwarded_https_headers() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-host",
        "goose.example.com".parse().expect("forwarded host"),
    );
    headers.insert(
        "x-forwarded-proto",
        "https".parse().expect("forwarded proto"),
    );
    headers.insert("host", "localhost:3000".parse().expect("host header"));

    assert_eq!(
        websocket_url(&headers),
        "wss://goose.example.com/api/agents/connect"
    );
}
