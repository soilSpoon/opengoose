mod loader;
mod selection;
#[cfg(test)]
mod tests;
mod view_model;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;
use urlencoding::encode;

use self::loader::load_session_records;
use self::selection::find_selected_session;
use crate::data::views::SessionsPageView;

pub(super) use self::loader::{SessionRecord, live_sessions, mock_sessions};
pub(super) use self::selection::choose_selected_session;
pub(super) use self::view_model::{
    build_batch_export_form, build_session_detail, build_session_list_items,
};

/// Load the sessions page view-model, optionally selecting a session by key.
pub fn load_sessions_page(db: Arc<Database>, selected: Option<String>) -> Result<SessionsPageView> {
    let loaded = load_session_records(db, 24)?;
    let selected_key = choose_selected_session(&loaded.sessions, selected);
    let selected_session = find_selected_session(&loaded.sessions, &selected_key)?;

    Ok(SessionsPageView {
        mode_label: loaded.mode.label.into(),
        mode_tone: loaded.mode.tone,
        live_stream_url: format!("/sessions/events?session={}", encode(&selected_key)),
        batch_export: build_batch_export_form(),
        sessions: build_session_list_items(
            &loaded.sessions,
            Some(selected_key.clone()),
            loaded.mode.label,
        ),
        selected: build_session_detail(selected_session, loaded.mode.label),
    })
}
