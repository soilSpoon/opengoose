use opengoose_types::SessionKey;

use crate::state::AppState;

pub(super) fn make_state() -> AppState {
    crate::handlers::test_support::make_state()
}

pub(super) fn append_user_message(
    state: &AppState,
    stable_id: &str,
    content: &str,
    author: Option<&str>,
) {
    let key = SessionKey::from_stable_id(stable_id);
    state
        .session_store
        .append_user_message(&key, content, author)
        .expect("append should succeed");
}

pub(super) fn append_assistant_message(state: &AppState, stable_id: &str, content: &str) {
    let key = SessionKey::from_stable_id(stable_id);
    state
        .session_store
        .append_assistant_message(&key, content)
        .expect("append should succeed");
}

pub(super) fn set_selected_model(state: &AppState, stable_id: &str, model: Option<&str>) {
    let key = SessionKey::from_stable_id(stable_id);
    state
        .session_store
        .set_selected_model(&key, model)
        .expect("selected model should persist");
}
