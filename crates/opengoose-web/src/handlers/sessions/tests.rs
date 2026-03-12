use axum::Json;
use axum::extract::{Path, Query, State};

use super::listing::default_limit;
use super::messages::default_msg_limit;
use super::test_support::{
    append_assistant_message, append_user_message, make_state, set_selected_model,
};
use super::{ListQuery, MessagesQuery, get_messages, list_sessions};

#[tokio::test]
async fn list_sessions_returns_empty_initially() {
    let state = make_state();
    let Json(sessions) = list_sessions(State(state), Query(ListQuery { limit: 50 }))
        .await
        .expect("list_sessions should succeed");
    assert!(sessions.is_empty());
}

#[tokio::test]
async fn list_sessions_returns_session_after_message_appended() {
    let state = make_state();
    append_user_message(
        &state,
        "discord:ns:guild123:chan456",
        "hello world",
        Some("alice"),
    );

    let Json(sessions) = list_sessions(State(state), Query(ListQuery { limit: 50 }))
        .await
        .expect("list_sessions should succeed");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_key, "discord:ns:guild123:chan456");
    assert!(sessions[0].selected_model.is_none());
}

#[tokio::test]
async fn list_sessions_respects_limit() {
    let state = make_state();
    for i in 0..5u32 {
        append_user_message(&state, &format!("slack:ns:team:ch{i}"), "msg", None);
    }

    let Json(sessions) = list_sessions(State(state), Query(ListQuery { limit: 3 }))
        .await
        .expect("list_sessions should succeed");

    assert_eq!(sessions.len(), 3);
}

#[tokio::test]
async fn list_sessions_includes_selected_model() {
    let state = make_state();
    append_user_message(
        &state,
        "discord:ns:guild123:chan456",
        "hello world",
        Some("alice"),
    );
    set_selected_model(&state, "discord:ns:guild123:chan456", Some("gpt-5-mini"));

    let Json(sessions) = list_sessions(State(state), Query(ListQuery { limit: 50 }))
        .await
        .expect("list_sessions should succeed");

    assert_eq!(sessions[0].selected_model.as_deref(), Some("gpt-5-mini"));
}

#[tokio::test]
async fn list_sessions_limit_zero_returns_error() {
    let state = make_state();
    let result = list_sessions(State(state), Query(ListQuery { limit: 0 })).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_sessions_limit_exceeds_max_returns_error() {
    let state = make_state();
    let result = list_sessions(State(state), Query(ListQuery { limit: 1001 })).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_sessions_limit_at_boundary_succeeds() {
    let state = make_state();
    assert!(
        list_sessions(State(state.clone()), Query(ListQuery { limit: 1 }))
            .await
            .is_ok()
    );
    assert!(
        list_sessions(State(state), Query(ListQuery { limit: 1000 }))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn get_messages_returns_empty_for_unknown_session() {
    let state = make_state();
    let Json(msgs) = get_messages(
        State(state),
        Path("unknown:session:key".into()),
        Query(MessagesQuery { limit: 100 }),
    )
    .await
    .expect("get_messages should succeed for empty session");
    assert!(msgs.is_empty());
}

#[tokio::test]
async fn get_messages_returns_messages_in_order() {
    let state = make_state();
    append_user_message(&state, "matrix:ns:room:abc", "first", Some("user1"));
    append_assistant_message(&state, "matrix:ns:room:abc", "second");

    let Json(msgs) = get_messages(
        State(state),
        Path("matrix:ns:room:abc".into()),
        Query(MessagesQuery { limit: 100 }),
    )
    .await
    .expect("get_messages should succeed");

    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].content, "first");
    assert_eq!(msgs[0].author.as_deref(), Some("user1"));
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].content, "second");
}

#[tokio::test]
async fn get_messages_respects_limit() {
    let state = make_state();
    for i in 0..10u32 {
        append_user_message(&state, "slack:ns:team:limited", &format!("msg {i}"), None);
    }

    let Json(msgs) = get_messages(
        State(state),
        Path("slack:ns:team:limited".into()),
        Query(MessagesQuery { limit: 4 }),
    )
    .await
    .expect("get_messages should succeed");

    assert_eq!(msgs.len(), 4);
}

#[tokio::test]
async fn get_messages_empty_session_key_returns_error() {
    let state = make_state();
    let result = get_messages(
        State(state),
        Path("   ".into()),
        Query(MessagesQuery { limit: 100 }),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_messages_limit_zero_returns_error() {
    let state = make_state();
    let result = get_messages(
        State(state),
        Path("any:session:key".into()),
        Query(MessagesQuery { limit: 0 }),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_messages_limit_exceeds_max_returns_error() {
    let state = make_state();
    let result = get_messages(
        State(state),
        Path("any:session:key".into()),
        Query(MessagesQuery { limit: 5001 }),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_messages_limit_at_boundary_succeeds() {
    let state = make_state();
    assert!(
        get_messages(
            State(state.clone()),
            Path("any:session:key".into()),
            Query(MessagesQuery { limit: 1 }),
        )
        .await
        .is_ok()
    );
    assert!(
        get_messages(
            State(state),
            Path("any:session:key".into()),
            Query(MessagesQuery { limit: 5000 }),
        )
        .await
        .is_ok()
    );
}

#[test]
fn list_query_default_limit_is_50() {
    assert_eq!(default_limit(), 50);
}

#[test]
fn messages_query_default_limit_is_100() {
    assert_eq!(default_msg_limit(), 100);
}
