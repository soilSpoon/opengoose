use opengoose_types::{AppEventKind, Platform, SessionKey};

use super::super::state::*;

#[test]
fn test_summarize_channel_disconnected_timeout() {
    let (summary, level, notice) =
        super::presentation::summarize_event(&AppEventKind::ChannelDisconnected {
            platform: Platform::Discord,
            reason: "socket timed out while connecting".into(),
        });

    assert_eq!(level, EventLevel::Error);
    assert_eq!(
        summary,
        "Gateway connection lost: discord gateway timed out. Check the network and try again."
    );
    assert_eq!(notice.unwrap(), summary);
}

#[test]
fn test_summarize_session_disconnected_connection_refused() {
    let (summary, level, notice) =
        super::presentation::summarize_event(&AppEventKind::SessionDisconnected {
            session_key: SessionKey::dm(Platform::Discord, "user-1"),
            reason: "connection refused".into(),
        });

    assert_eq!(level, EventLevel::Error);
    assert_eq!(
        summary,
        "Session disconnected: discord:user-1 refused the connection."
    );
    assert_eq!(notice.unwrap(), summary);
}

#[test]
fn test_summarize_error_event_uses_humanized_messages() {
    let (summary, level, notice) = super::presentation::summarize_event(&AppEventKind::Error {
        context: "ui".into(),
        message: "events dropped due to lag while rendering".into(),
    });

    assert_eq!(level, EventLevel::Error);
    assert_eq!(
        summary,
        "The TUI fell behind and dropped some updates. Resize the terminal or reduce log volume."
    );
    assert_eq!(notice.unwrap(), summary);

    let (summary, level, notice) = super::presentation::summarize_event(&AppEventKind::Error {
        context: "gateway".into(),
        message: "request timed out after 3s".into(),
    });
    assert_eq!(level, EventLevel::Error);
    assert_eq!(summary, "gateway: the request timed out. Please retry.");
    assert_eq!(notice.unwrap(), summary);
}

#[test]
fn test_summarize_stream_started_uses_session_label() {
    let (summary, level, notice) =
        super::presentation::summarize_event(&AppEventKind::StreamStarted {
            session_key: SessionKey::dm(Platform::Discord, "dm-1"),
            stream_id: "stream-1".into(),
        });

    assert_eq!(level, EventLevel::Info);
    assert_eq!(summary, "Agent is thinking for discord:dm-1.");
    assert!(notice.is_none());
}

#[test]
fn test_summarize_default_event_has_no_notice() {
    let (summary, level, notice) =
        super::presentation::summarize_event(&AppEventKind::DashboardUpdated);

    assert_eq!(level, EventLevel::Info);
    assert_eq!(summary, "dashboard updated");
    assert!(notice.is_none());
}
