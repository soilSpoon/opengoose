use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};

use super::AppError;
use crate::state::AppState;

/// JSON response item for a single chat session.
#[derive(Serialize)]
pub struct SessionItem {
    pub session_key: String,
    pub active_team: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Query parameters for `GET /api/sessions`.
#[derive(Deserialize)]
pub struct ListQuery {
    /// Maximum number of sessions to return (default 50, max 1000).
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// GET /api/sessions — list recent chat sessions.
pub async fn list_sessions(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<SessionItem>>, AppError> {
    if q.limit <= 0 || q.limit > 1000 {
        return Err(AppError::UnprocessableEntity(format!(
            "`limit` must be between 1 and 1000, got {}",
            q.limit
        )));
    }
    let sessions = state.session_store.list_sessions(q.limit)?;
    Ok(Json(
        sessions
            .into_iter()
            .map(|s| SessionItem {
                session_key: s.session_key,
                active_team: s.active_team,
                created_at: s.created_at,
                updated_at: s.updated_at,
            })
            .collect(),
    ))
}

/// JSON response item for a single chat message.
#[derive(Serialize)]
pub struct MessageItem {
    pub role: String,
    pub content: String,
    pub author: Option<String>,
    pub created_at: String,
}

/// Query parameters for `GET /api/sessions/{session_key}/messages`.
#[derive(Deserialize)]
pub struct MessagesQuery {
    /// Maximum number of messages to return (default 100, max 5000).
    #[serde(default = "default_msg_limit")]
    pub limit: usize,
}

fn default_msg_limit() -> usize {
    100
}

/// GET /api/sessions/{session_key}/messages — list messages for a session.
pub async fn get_messages(
    State(state): State<AppState>,
    Path(session_key): Path<String>,
    Query(q): Query<MessagesQuery>,
) -> Result<Json<Vec<MessageItem>>, AppError> {
    if session_key.trim().is_empty() {
        return Err(AppError::BadRequest(
            "`session_key` must not be empty".into(),
        ));
    }
    if q.limit == 0 || q.limit > 5000 {
        return Err(AppError::UnprocessableEntity(format!(
            "`limit` must be between 1 and 5000, got {}",
            q.limit
        )));
    }
    use opengoose_types::SessionKey;
    // Accept raw stable-id strings directly (e.g. "discord:guild:channel")
    let key = SessionKey::from_stable_id(&session_key);
    let messages = state.session_store.load_history(&key, q.limit)?;
    Ok(Json(
        messages
            .into_iter()
            .map(|m| MessageItem {
                role: m.role,
                content: m.content,
                author: m.author,
                created_at: m.created_at,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::Json;
    use axum::extract::{Path, Query, State};
    use opengoose_persistence::{
        AlertStore, Database, OrchestrationStore, ScheduleStore, SessionStore, TriggerStore,
    };
    use opengoose_profiles::ProfileStore;
    use opengoose_teams::TeamStore;
    use opengoose_types::{ChannelMetricsStore, SessionKey};

    use super::{ListQuery, MessagesQuery, get_messages, list_sessions};
    use crate::state::AppState;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "opengoose-web-sessions-{label}-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp test dir should be created");
        dir
    }

    fn make_state() -> AppState {
        let db = Arc::new(Database::open_in_memory().expect("in-memory db should open"));
        AppState {
            db: db.clone(),
            session_store: Arc::new(SessionStore::new(db.clone())),
            orchestration_store: Arc::new(OrchestrationStore::new(db.clone())),
            profile_store: Arc::new(ProfileStore::with_dir(unique_temp_dir("profiles"))),
            team_store: Arc::new(TeamStore::with_dir(unique_temp_dir("teams"))),
            schedule_store: Arc::new(ScheduleStore::new(db.clone())),
            trigger_store: Arc::new(TriggerStore::new(db.clone())),
            alert_store: Arc::new(AlertStore::new(db)),
            channel_metrics: ChannelMetricsStore::new(),
        }
    }

    // ---- list_sessions ----

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
        let key = SessionKey::from_stable_id("discord:ns:guild123:chan456");
        state
            .session_store
            .append_user_message(&key, "hello world", Some("alice"))
            .expect("append should succeed");

        let Json(sessions) = list_sessions(State(state), Query(ListQuery { limit: 50 }))
            .await
            .expect("list_sessions should succeed");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_key, "discord:ns:guild123:chan456");
    }

    #[tokio::test]
    async fn list_sessions_respects_limit() {
        let state = make_state();
        for i in 0..5u32 {
            let key = SessionKey::from_stable_id(&format!("slack:ns:team:ch{i}"));
            state
                .session_store
                .append_user_message(&key, "msg", None)
                .expect("append should succeed");
        }

        let Json(sessions) = list_sessions(State(state), Query(ListQuery { limit: 3 }))
            .await
            .expect("list_sessions should succeed");

        assert_eq!(sessions.len(), 3);
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
        // limit=1 and limit=1000 are both valid
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

    // ---- get_messages ----

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
        let key = SessionKey::from_stable_id("matrix:ns:room:abc");
        state
            .session_store
            .append_user_message(&key, "first", Some("user1"))
            .unwrap();
        state
            .session_store
            .append_assistant_message(&key, "second")
            .unwrap();

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
        let key = SessionKey::from_stable_id("slack:ns:team:limited");
        for i in 0..10u32 {
            state
                .session_store
                .append_user_message(&key, &format!("msg {i}"), None)
                .unwrap();
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

    // ---- default query parameter values ----

    #[test]
    fn list_query_default_limit_is_50() {
        assert_eq!(super::default_limit(), 50);
    }

    #[test]
    fn messages_query_default_limit_is_100() {
        assert_eq!(super::default_msg_limit(), 100);
    }
}
