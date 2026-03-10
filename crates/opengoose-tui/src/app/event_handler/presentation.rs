use opengoose_types::AppEventKind;

use super::super::state::*;

pub(super) fn summarize_event(kind: &AppEventKind) -> (String, EventLevel, Option<String>) {
    match kind {
        AppEventKind::ChannelDisconnected { platform, reason } => {
            let summary = humanize_disconnect(
                &format!("{} gateway", platform.as_str()),
                reason,
                "Gateway connection lost",
            );
            (summary.clone(), EventLevel::Error, Some(summary))
        }
        AppEventKind::SessionDisconnected {
            session_key,
            reason,
        } => {
            let summary = humanize_disconnect(
                &App::format_session_label(session_key),
                reason,
                "Session disconnected",
            );
            (summary.clone(), EventLevel::Error, Some(summary))
        }
        AppEventKind::Error { context, message } => {
            let summary = humanize_error(context, message);
            (summary.clone(), EventLevel::Error, Some(summary))
        }
        AppEventKind::StreamStarted { session_key, .. } => (
            format!(
                "Agent is thinking for {}.",
                App::format_session_label(session_key)
            ),
            EventLevel::Info,
            None,
        ),
        AppEventKind::StreamUpdated {
            session_key,
            content_len,
            ..
        } => (
            format!(
                "Agent is generating a response for {} ({} chars).",
                App::format_session_label(session_key),
                content_len
            ),
            EventLevel::Info,
            None,
        ),
        AppEventKind::StreamCompleted { session_key, .. } => (
            format!(
                "Agent finished responding in {}.",
                App::format_session_label(session_key)
            ),
            EventLevel::Info,
            None,
        ),
        _ => (kind.to_string(), EventLevel::Info, None),
    }
}

fn humanize_disconnect(subject: &str, reason: &str, prefix: &str) -> String {
    let lowered = reason.to_ascii_lowercase();
    if lowered.contains("timeout") || lowered.contains("timed out") {
        return format!("{prefix}: {subject} timed out. Check the network and try again.");
    }
    if lowered.contains("connection refused") {
        return format!("{prefix}: {subject} refused the connection.");
    }
    if lowered.contains("dns") || lowered.contains("resolve") {
        return format!("{prefix}: {subject} could not be resolved.");
    }
    format!("{prefix}: {subject} ({reason}).")
}

fn humanize_error(context: &str, message: &str) -> String {
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("events dropped due to lag") {
        return "The TUI fell behind and dropped some updates. Resize the terminal or reduce log volume."
            .to_string();
    }
    if lowered.contains("timeout") || lowered.contains("timed out") {
        return format!("{context}: the request timed out. Please retry.");
    }
    if lowered.contains("connection refused") {
        return format!("{context}: the target service refused the connection.");
    }
    if lowered.contains("broken pipe") {
        return format!("{context}: the connection closed unexpectedly.");
    }
    format!("{context}: {message}")
}
