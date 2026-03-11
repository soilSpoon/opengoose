use anyhow::{Context, Result};

use super::loader::SessionRecord;

pub(in crate::data) fn choose_selected_session(
    sessions: &[SessionRecord],
    selected: Option<String>,
) -> String {
    selected
        .filter(|target| {
            sessions
                .iter()
                .any(|session| session.summary.session_key == *target)
        })
        .unwrap_or_else(|| sessions[0].summary.session_key.clone())
}

pub(in crate::data) fn find_selected_session<'a>(
    sessions: &'a [SessionRecord],
    selected_key: &str,
) -> Result<&'a SessionRecord> {
    sessions
        .iter()
        .find(|session| session.summary.session_key == selected_key)
        .context("selected session missing")
}
