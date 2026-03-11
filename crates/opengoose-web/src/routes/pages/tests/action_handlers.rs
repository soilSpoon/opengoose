use axum::body::Body;
use axum::extract::{Form, State};
use axum::http::{Method, Request, StatusCode};
use axum::response::Html;
use opengoose_persistence::{ScheduleStore, TriggerStore};
use tower::ServiceExt;

use super::super::catalog::{schedule_action, team_save, trigger_action};
use super::super::catalog_forms::{ScheduleActionForm, TeamSaveForm, TriggerActionForm};
use super::super::router;
use super::support::{TEMP_HOME_PREFIX, page_state, read_body, run_async, save_team, test_db};
use crate::test_support::with_temp_home;

#[tokio::test]
async fn trigger_action_create_renders_notice_and_new_trigger() {
    let db = test_db();

    let Html(html) = trigger_action(
        State(page_state(db.clone())),
        Form(TriggerActionForm {
            intent: "create".into(),
            original_name: None,
            name: Some("on-pr".into()),
            trigger_type: Some("webhook_received".into()),
            team_name: Some("code-review".into()),
            condition_json: Some(r#"{"path":"/pr"}"#.into()),
            input: Some("review".into()),
        }),
    )
    .await
    .expect("create action should render");

    assert!(html.contains("Trigger `on-pr` created."));
    assert!(html.contains("on-pr"));
    assert!(
        TriggerStore::new(db)
            .get_by_name("on-pr")
            .expect("lookup should succeed")
            .is_some()
    );
}

#[test]
fn team_save_invalid_yaml_renders_editor_error_notice() {
    with_temp_home(TEMP_HOME_PREFIX, || {
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
    with_temp_home(TEMP_HOME_PREFIX, || {
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
fn schedule_action_creates_schedule_from_form_post() {
    with_temp_home(TEMP_HOME_PREFIX, || {
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
    with_temp_home(TEMP_HOME_PREFIX, || {
        run_async(async {
            let response = router(page_state(test_db()))
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
