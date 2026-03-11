use axum::body::Body;
use axum::extract::{Form, Query, State};
use axum::http::{Method, Request, StatusCode};
use axum::response::Html;
use opengoose_persistence::ScheduleStore;
use tower::ServiceExt;

use super::super::support::{
    page_state, read_body, run_async, save_team, test_db, with_pages_home,
};
use super::{ScheduleActionForm, ScheduleQuery, schedule_action, schedules};

#[test]
fn schedule_action_missing_team_renders_validation_notice() {
    with_pages_home(|| {
        run_async(async {
            let Html(html) = schedule_action(
                State(page_state(test_db())),
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
    with_pages_home(|| {
        save_team("ops");
        let db = test_db();
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
    with_pages_home(|| {
        save_team("ops");
        let db = test_db();

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
    with_pages_home(|| {
        run_async(async {
            let response = super::super::super::router(page_state(test_db()))
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
