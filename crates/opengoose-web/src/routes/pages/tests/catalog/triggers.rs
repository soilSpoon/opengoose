use axum::extract::{Form, Query, State};
use axum::response::Html;
use opengoose_persistence::TriggerStore;

use super::super::support::{page_state, test_db};
use super::{TriggerActionForm, TriggerQuery, trigger_action, triggers};

#[tokio::test]
async fn triggers_handler_invalid_selection_falls_back_to_existing_trigger() {
    let db = test_db();
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
