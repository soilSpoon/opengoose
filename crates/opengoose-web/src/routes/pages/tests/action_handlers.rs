use axum::body::Body;
use axum::extract::{Form, State};
use axum::http::{Method, Request, StatusCode};
use axum::response::Html;
use opengoose_persistence::{PluginStore, ScheduleStore, SessionStore, TriggerStore};
use opengoose_types::SessionKey;
use tower::ServiceExt;

use super::super::catalog::{
    plugin_action, schedule_action, session_action, team_save, trigger_action,
};
use super::super::catalog_forms::{
    PluginActionForm, ScheduleActionForm, SessionActionForm, TeamSaveForm, TriggerActionForm,
};
use super::super::router;
use super::support::{
    TEMP_HOME_PREFIX, page_state, read_body, run_async, save_session, save_team, test_db,
    write_plugin_manifest,
};
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

#[tokio::test]
async fn trigger_action_update_renders_notice_and_updates_existing_trigger() {
    let db = test_db();
    TriggerStore::new(db.clone())
        .create(
            "on-pr",
            "webhook_received",
            r#"{"path":"/pr"}"#,
            "code-review",
            "review",
        )
        .expect("trigger should seed");

    let Html(html) = trigger_action(
        State(page_state(db.clone())),
        Form(TriggerActionForm {
            intent: "update".into(),
            original_name: Some("on-pr".into()),
            name: None,
            trigger_type: Some("file_watch".into()),
            team_name: Some("incident-response".into()),
            condition_json: Some(r#"{"path":"/repo"}"#.into()),
            input: Some("watch".into()),
        }),
    )
    .await
    .expect("update action should render");

    assert!(html.contains("Trigger `on-pr` saved."));
    let trigger = TriggerStore::new(db)
        .get_by_name("on-pr")
        .expect("lookup should succeed")
        .expect("trigger should exist");
    assert_eq!(trigger.trigger_type, "file_watch");
    assert_eq!(trigger.team_name, "incident-response");
    assert_eq!(trigger.condition_json, r#"{"path":"/repo"}"#);
    assert_eq!(trigger.input, "watch");
}

#[tokio::test]
async fn trigger_action_toggle_renders_notice_and_disables_trigger() {
    let db = test_db();
    TriggerStore::new(db.clone())
        .create(
            "on-pr",
            "webhook_received",
            r#"{"path":"/pr"}"#,
            "code-review",
            "review",
        )
        .expect("trigger should seed");

    let Html(html) = trigger_action(
        State(page_state(db.clone())),
        Form(TriggerActionForm {
            intent: "toggle".into(),
            original_name: Some("on-pr".into()),
            name: None,
            trigger_type: None,
            team_name: None,
            condition_json: None,
            input: None,
        }),
    )
    .await
    .expect("toggle action should render");

    assert!(html.contains("Trigger `on-pr` disabled."));
    assert!(
        !TriggerStore::new(db)
            .get_by_name("on-pr")
            .expect("lookup should succeed")
            .expect("trigger should exist")
            .enabled
    );
}

#[tokio::test]
async fn trigger_action_delete_renders_notice_and_removes_trigger() {
    let db = test_db();
    TriggerStore::new(db.clone())
        .create(
            "on-pr",
            "webhook_received",
            r#"{"path":"/pr"}"#,
            "code-review",
            "review",
        )
        .expect("trigger should seed");

    let Html(html) = trigger_action(
        State(page_state(db.clone())),
        Form(TriggerActionForm {
            intent: "delete".into(),
            original_name: Some("on-pr".into()),
            name: None,
            trigger_type: None,
            team_name: None,
            condition_json: None,
            input: None,
        }),
    )
    .await
    .expect("delete action should render");

    assert!(html.contains("Trigger `on-pr` deleted."));
    assert!(
        TriggerStore::new(db)
            .get_by_name("on-pr")
            .expect("lookup should succeed")
            .is_none()
    );
}

#[tokio::test]
async fn trigger_action_test_renders_queue_notice() {
    let db = test_db();
    TriggerStore::new(db.clone())
        .create(
            "on-pr",
            "webhook_received",
            r#"{"path":"/pr"}"#,
            "code-review",
            "review",
        )
        .expect("trigger should seed");

    let Html(html) = trigger_action(
        State(page_state(db)),
        Form(TriggerActionForm {
            intent: "test".into(),
            original_name: Some("on-pr".into()),
            name: None,
            trigger_type: None,
            team_name: None,
            condition_json: None,
            input: None,
        }),
    )
    .await
    .expect("test action should render");

    assert!(html.contains("Trigger `on-pr` test queued. Check Runs for progress."));
}

#[tokio::test]
async fn plugin_action_install_validation_error_renders_notice() {
    let Html(html) = plugin_action(
        State(page_state(test_db())),
        Form(PluginActionForm {
            intent: "install".into(),
            original_name: None,
            source_path: Some(String::new()),
            confirm_delete: None,
        }),
    )
    .await
    .expect("handler should render");

    assert!(html.contains("Plugin path is required."));
    assert!(html.contains("Install plugin"));
}

#[test]
fn plugin_action_install_creates_plugin_from_form_post() {
    with_temp_home("opengoose-routes-pages-plugin-install-home", || {
        let tmp = tempfile::tempdir().expect("temp dir should build");
        let plugin_dir = write_plugin_manifest(tmp.path(), "ops-tools", "1.2.3");
        let db = test_db();

        run_async(async {
            let Html(html) = plugin_action(
                State(page_state(db.clone())),
                Form(PluginActionForm {
                    intent: "install".into(),
                    original_name: None,
                    source_path: Some(plugin_dir.display().to_string()),
                    confirm_delete: None,
                }),
            )
            .await
            .expect("install action should render");

            assert!(html.contains("Installed plugin `ops-tools`."));
            assert!(html.contains("ops-tools"));
            assert!(
                PluginStore::new(db)
                    .get_by_name("ops-tools")
                    .expect("lookup should succeed")
                    .is_some()
            );
        });
    });
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

#[test]
fn trigger_action_unsupported_intent_returns_bad_request() {
    with_temp_home(TEMP_HOME_PREFIX, || {
        run_async(async {
            let response = router(page_state(test_db()))
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/triggers")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("intent=unsupported"))
                        .unwrap(),
                )
                .await
                .expect("request should be handled");

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let html = read_body(response).await;
            assert!(html.contains("Unsupported trigger action."));
        });
    });
}

#[tokio::test]
async fn session_action_save_sets_model_override_and_notice() {
    let db = test_db();
    let session_key = SessionKey::from_stable_id("discord:ns:ops:chan-1");
    save_session(db.clone(), &session_key, Some("ops"));

    let Html(html) = session_action(
        State(page_state(db.clone())),
        Form(SessionActionForm {
            intent: "save".into(),
            session_key: session_key.to_stable_id(),
            selected_model: Some("gpt-5-mini".into()),
        }),
    )
    .await
    .expect("save action should render");

    assert!(html.contains("Model override set to `gpt-5-mini`."));
    assert_eq!(
        SessionStore::new(db)
            .get_selected_model(&session_key)
            .expect("lookup should succeed")
            .as_deref(),
        Some("gpt-5-mini")
    );
}
