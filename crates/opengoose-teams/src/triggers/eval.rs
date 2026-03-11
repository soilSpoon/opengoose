//! Condition evaluation / matching functions for triggers.

use std::path::Path;

use super::types::{
    FileWatchCondition, MessageCondition, OnMessageCondition, OnScheduleCondition,
    OnSessionCondition, WebhookCondition,
};

/// Check whether a `WebhookReceived` trigger condition matches the given path.
///
/// The path from the trigger condition is treated as a prefix. A trigger with
/// no path configured matches every incoming webhook path.
pub fn matches_webhook_path(condition_json: &str, path: &str) -> bool {
    let cond: WebhookCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };
    match cond.path {
        None => true,
        Some(ref p) => path.starts_with(p.as_str()),
    }
}

/// Check whether an `OnMessage` trigger matches a `MessageReceived` event.
pub fn matches_on_message_event(condition_json: &str, author: &str, content: &str) -> bool {
    let cond: OnMessageCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref expected) = cond.from_author
        && expected != author
    {
        return false;
    }

    if let Some(ref needle) = cond.content_contains
        && !content.contains(needle.as_str())
    {
        return false;
    }

    true
}

/// Check whether an `OnSessionStart`/`OnSessionEnd` trigger matches.
pub fn matches_on_session_event(condition_json: &str, platform: &str) -> bool {
    let cond: OnSessionCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref expected) = cond.platform
        && expected != platform
    {
        return false;
    }

    true
}

/// Check whether an `OnSchedule` trigger matches a `TeamRunCompleted` event.
pub fn matches_on_schedule_event(condition_json: &str, completed_team: &str) -> bool {
    let cond: OnScheduleCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref expected) = cond.team
        && expected != completed_team
    {
        return false;
    }

    true
}

/// Check whether a `FileWatch` condition matches a file path.
///
/// An empty pattern (or missing condition) matches all paths.
pub fn matches_file_watch_event(condition_json: &str, path: &str) -> bool {
    let cond: FileWatchCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let pattern = match &cond.pattern {
        Some(p) => p,
        // No pattern set — match everything.
        None => return true,
    };

    let glob = match globset::Glob::new(pattern) {
        Ok(g) => g.compile_matcher(),
        Err(_) => return false,
    };

    glob.is_match(Path::new(path))
}

/// Check whether a `MessageReceived` trigger matches a bus event.
pub fn matches_message_event(
    condition_json: &str,
    from: &str,
    channel: Option<&str>,
    payload: &str,
) -> bool {
    let cond: MessageCondition = match serde_json::from_str(condition_json) {
        Ok(c) => c,
        Err(_) => return false,
    };

    if let Some(ref expected_from) = cond.from_agent
        && expected_from != from
    {
        return false;
    }

    if let Some(ref expected_channel) = cond.channel {
        match channel {
            Some(ch) if ch == expected_channel => {}
            _ => return false,
        }
    }

    if let Some(ref needle) = cond.payload_contains
        && !payload.contains(needle.as_str())
    {
        return false;
    }

    true
}

pub(crate) fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..s.floor_char_boundary(max)]
    }
}
