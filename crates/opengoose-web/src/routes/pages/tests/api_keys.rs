use axum::extract::{Form, State};
use axum::response::Html;
use opengoose_persistence::ApiKeyStore;

use super::super::api_keys::{ApiKeyActionForm, api_key_action, api_keys};
use super::support::{page_state, test_db};

#[tokio::test]
async fn api_keys_handler_renders_empty_state() {
    let Html(html) = api_keys(State(page_state(test_db())))
        .await
        .expect("handler should render");

    assert!(html.contains("No API keys have been generated yet."));
    assert!(html.contains("Generate a new API key"));
    assert!(html.contains("aria-current=\"page\""));
}

#[tokio::test]
async fn api_key_action_generate_reveals_plaintext_once() {
    let db = test_db();

    let Html(html) = api_key_action(
        State(page_state(db.clone())),
        Form(ApiKeyActionForm {
            intent: "generate".into(),
            description: Some("CI pipeline".into()),
            key_id: None,
        }),
    )
    .await
    .expect("generate action should render");

    assert!(html.contains("API key reveal"));
    assert!(html.contains("Save this API key now"));
    assert!(html.contains("ogk_"));
    assert!(html.contains("CI pipeline"));
    assert_eq!(
        ApiKeyStore::new(db)
            .list()
            .expect("list should succeed")
            .len(),
        1
    );
}

#[tokio::test]
async fn api_key_action_revoke_removes_key() {
    let db = test_db();
    let seeded_key = ApiKeyStore::new(db.clone())
        .generate(Some("remote-agent"))
        .expect("key should seed");

    let Html(html) = api_key_action(
        State(page_state(db.clone())),
        Form(ApiKeyActionForm {
            intent: "revoke".into(),
            description: None,
            key_id: Some(seeded_key.id.clone()),
        }),
    )
    .await
    .expect("revoke action should render");

    assert!(html.contains("revoked"));
    assert!(html.contains("No API keys have been generated yet."));
    assert!(
        ApiKeyStore::new(db)
            .list()
            .expect("list should succeed")
            .is_empty()
    );
}

#[tokio::test]
async fn api_key_action_revoke_missing_key_renders_failure_notice() {
    let Html(html) = api_key_action(
        State(page_state(test_db())),
        Form(ApiKeyActionForm {
            intent: "revoke".into(),
            description: None,
            key_id: Some("missing-key".into()),
        }),
    )
    .await
    .expect("missing revoke should render");

    assert!(html.contains("was not found"));
    assert!(html.contains("No API keys have been generated yet."));
}
