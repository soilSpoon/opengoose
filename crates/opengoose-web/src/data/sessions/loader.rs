use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, HistoryMessage, SessionStore, SessionSummary};
use opengoose_types::SessionKey;

#[derive(Clone, Debug)]
pub(in crate::data) struct SessionRecord {
    pub(in crate::data) summary: SessionSummary,
    pub(in crate::data) messages: Vec<HistoryMessage>,
}

#[derive(Clone, Copy)]
pub(in crate::data) struct SessionDataMode {
    pub(super) label: &'static str,
    pub(super) tone: &'static str,
}

pub(in crate::data) struct LoadedSessions {
    pub(super) mode: SessionDataMode,
    pub(super) sessions: Vec<SessionRecord>,
}

const LIVE_RUNTIME_MODE: SessionDataMode = SessionDataMode {
    label: "Live runtime",
    tone: "success",
};

const MOCK_PREVIEW_MODE: SessionDataMode = SessionDataMode {
    label: "Mock preview",
    tone: "neutral",
};

pub(in crate::data) fn load_session_records(
    db: Arc<Database>,
    limit: i64,
) -> Result<LoadedSessions> {
    let store = SessionStore::new(db);
    let session_rows = store.list_sessions(limit)?;

    if session_rows.is_empty() {
        Ok(LoadedSessions {
            mode: MOCK_PREVIEW_MODE,
            sessions: mock_sessions(),
        })
    } else {
        Ok(LoadedSessions {
            mode: LIVE_RUNTIME_MODE,
            sessions: live_sessions(&store, &session_rows)?,
        })
    }
}

pub(in crate::data) fn live_sessions(
    store: &SessionStore,
    rows: &[SessionSummary],
) -> Result<Vec<SessionRecord>> {
    rows.iter()
        .map(|summary| {
            let key = SessionKey::from_stable_id(&summary.session_key);
            Ok(SessionRecord {
                summary: summary.clone(),
                messages: store.load_history(&key, 40)?,
            })
        })
        .collect()
}

pub(in crate::data) fn mock_sessions() -> Vec<SessionRecord> {
    vec![
        SessionRecord {
            summary: SessionSummary {
                session_key: "discord:ns:studio-a:ops-bridge".into(),
                active_team: Some("feature-dev".into()),
                selected_model: Some("claude-sonnet-4-20250514".into()),
                created_at: "2026-03-10 09:00".into(),
                updated_at: "2026-03-10 10:28".into(),
            },
            messages: vec![
                HistoryMessage {
                    role: "user".into(),
                    content: "Spin up a reviewer and confirm the deploy checklist.".into(),
                    author: Some("pm-sora".into()),
                    created_at: "2026-03-10 10:11".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content:
                        "Feature-dev is active. Routing implementation notes to reviewer next."
                            .into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 10:12".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content:
                        "Reviewer flagged one missing migration note. Queue updated for follow-up."
                            .into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 10:28".into(),
                },
            ],
        },
        SessionRecord {
            summary: SessionSummary {
                session_key: "telegram:direct:founder-42".into(),
                active_team: None,
                selected_model: None,
                created_at: "2026-03-10 08:21".into(),
                updated_at: "2026-03-10 09:44".into(),
            },
            messages: vec![
                HistoryMessage {
                    role: "user".into(),
                    content: "Summarize the backlog movement from this morning.".into(),
                    author: Some("founder".into()),
                    created_at: "2026-03-10 09:40".into(),
                },
                HistoryMessage {
                    role: "assistant".into(),
                    content:
                        "Three frontend issues advanced to implementation, one queue alert remains unresolved."
                            .into(),
                    author: Some("goose".into()),
                    created_at: "2026-03-10 09:44".into(),
                },
            ],
        },
    ]
}
