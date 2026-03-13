use super::*;
use crate::{Platform, SessionKey};

fn sample_session_key() -> SessionKey {
    SessionKey::new(Platform::Discord, "g1", "ch1")
}

fn sample_app_event_kinds() -> Vec<AppEventKind> {
    let key = sample_session_key();

    vec![
        AppEventKind::GooseReady,
        AppEventKind::ChannelReady {
            platform: Platform::Discord,
        },
        AppEventKind::ChannelDisconnected {
            platform: Platform::Slack,
            reason: "network timeout".into(),
        },
        AppEventKind::ChannelReconnecting {
            platform: Platform::Telegram,
            attempt: 2,
            delay_secs: 15,
        },
        AppEventKind::MessageReceived {
            session_key: key.clone(),
            author: "alice".into(),
            content: "hello".into(),
        },
        AppEventKind::ResponseSent {
            session_key: key.clone(),
            content: "world".into(),
        },
        AppEventKind::PairingCodeGenerated {
            code: "ABC123".into(),
        },
        AppEventKind::PairingCompleted {
            session_key: key.clone(),
        },
        AppEventKind::TeamActivated {
            session_key: key.clone(),
            team_name: "review".into(),
        },
        AppEventKind::TeamDeactivated {
            session_key: key.clone(),
        },
        AppEventKind::SessionDisconnected {
            session_key: key.clone(),
            reason: "idle".into(),
        },
        AppEventKind::Error {
            context: "bridge".into(),
            message: "boom".into(),
        },
        AppEventKind::TracingEvent {
            level: "INFO".into(),
            message: "trace".into(),
        },
        AppEventKind::DashboardUpdated,
        AppEventKind::SessionUpdated {
            session_key: key.clone(),
        },
        AppEventKind::RunUpdated {
            team_run_id: "run-1".into(),
            status: "running".into(),
        },
        AppEventKind::QueueUpdated {
            team_run_id: Some("run-1".into()),
        },
        AppEventKind::StreamStarted {
            session_key: key.clone(),
            stream_id: "stream-1".into(),
        },
        AppEventKind::StreamUpdated {
            session_key: key.clone(),
            stream_id: "stream-1".into(),
            content_len: 128,
        },
        AppEventKind::StreamCompleted {
            session_key: key.clone(),
            stream_id: "stream-1".into(),
            full_text: "done".into(),
        },
        AppEventKind::TeamRunStarted {
            team: "review".into(),
            workflow: "chain".into(),
            input: "input".into(),
        },
        AppEventKind::TeamStepStarted {
            team: "review".into(),
            agent: "coder".into(),
            step: 1,
        },
        AppEventKind::TeamStepCompleted {
            team: "review".into(),
            agent: "coder".into(),
        },
        AppEventKind::TeamStepFailed {
            team: "review".into(),
            agent: "coder".into(),
            reason: "timeout".into(),
        },
        AppEventKind::TeamRunCompleted {
            team: "review".into(),
        },
        AppEventKind::TeamRunFailed {
            team: "review".into(),
            reason: "timeout".into(),
        },
        AppEventKind::AlertFired {
            rule_name: "high-latency".into(),
            metric: "latency_ms".into(),
            value: 250.0,
            platform: "discord".into(),
            channel_id: "ops-room".into(),
        },
        AppEventKind::ModelChanged {
            session_key: key.clone(),
            model: "gpt-5".into(),
            mode: "auto".into(),
        },
        AppEventKind::ContextCompacted {
            session_key: key.clone(),
        },
        AppEventKind::ExtensionNotification {
            session_key: key.clone(),
            extension: "developer".into(),
        },
        AppEventKind::ShutdownStarted {
            timeout_secs: 30,
            active_streams: 4,
        },
        AppEventKind::ShutdownCompleted {
            timed_out: false,
            remaining_streams: 0,
        },
    ]
}

#[tokio::test]
async fn test_event_bus_emit_subscribe() {
    let bus = EventBus::new(16);
    let mut rx = bus.subscribe();
    bus.emit(AppEventKind::ChannelReady {
        platform: Platform::Discord,
    });
    let event = rx.recv().await.unwrap();
    assert!(matches!(
        event.kind,
        AppEventKind::ChannelReady {
            platform: Platform::Discord
        }
    ));
}

#[test]
fn test_event_bus_no_subscribers_no_panic() {
    let bus = EventBus::new(16);
    bus.emit(AppEventKind::ChannelReady {
        platform: Platform::Discord,
    });
}

#[tokio::test]
async fn test_event_bus_multiple_subscribers() {
    let bus = EventBus::new(16);
    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();
    bus.emit(AppEventKind::ChannelReady {
        platform: Platform::Discord,
    });
    let e1 = rx1.recv().await.unwrap();
    let e2 = rx2.recv().await.unwrap();
    assert!(matches!(
        e1.kind,
        AppEventKind::ChannelReady {
            platform: Platform::Discord
        }
    ));
    assert!(matches!(
        e2.kind,
        AppEventKind::ChannelReady {
            platform: Platform::Discord
        }
    ));
}

#[tokio::test]
async fn test_event_bus_reliable_subscription_receives_event() {
    let bus = EventBus::new(1);
    let mut rx = bus.subscribe_reliable();

    bus.emit(AppEventKind::GooseReady);

    let event = rx.recv().await.expect("event should arrive");
    assert_eq!(event.kind.key(), "goose_ready");
}

#[test]
fn test_app_event_kind_display() {
    assert_eq!(
        AppEventKind::ChannelReady {
            platform: Platform::Discord
        }
        .to_string(),
        "discord ready"
    );
    assert_eq!(
        AppEventKind::ChannelDisconnected {
            platform: Platform::Discord,
            reason: "bye".into()
        }
        .to_string(),
        "discord disconnected: bye"
    );
    assert_eq!(
        AppEventKind::PairingCodeGenerated {
            code: "ABC123".into()
        }
        .to_string(),
        "pairing code: ABC123"
    );
    assert_eq!(
        AppEventKind::Error {
            context: "test".into(),
            message: "fail".into()
        }
        .to_string(),
        "error [test]: fail"
    );
}

#[test]
fn test_app_event_kind_display_all_variants() {
    let key = SessionKey::new(Platform::Discord, "g1", "ch1");

    assert_eq!(
        AppEventKind::GooseReady.to_string(),
        "goose agent system ready"
    );

    assert_eq!(
        AppEventKind::MessageReceived {
            session_key: key.clone(),
            author: "alice".into(),
            content: "hi".into(),
        }
        .to_string(),
        "message from alice"
    );

    assert_eq!(
        AppEventKind::ResponseSent {
            session_key: key.clone(),
            content: "reply".into(),
        }
        .to_string(),
        "response sent"
    );

    assert_eq!(
        AppEventKind::PairingCompleted {
            session_key: key.clone(),
        }
        .to_string(),
        format!("paired: {key}")
    );

    assert_eq!(
        AppEventKind::TeamActivated {
            session_key: key.clone(),
            team_name: "review".into(),
        }
        .to_string(),
        format!("team activated: review on {key}")
    );

    assert_eq!(
        AppEventKind::TeamDeactivated {
            session_key: key.clone(),
        }
        .to_string(),
        format!("team deactivated on {key}")
    );

    assert_eq!(
        AppEventKind::SessionDisconnected {
            session_key: key.clone(),
            reason: "timeout".into(),
        }
        .to_string(),
        format!("session disconnected: {key} (timeout)")
    );

    assert_eq!(
        AppEventKind::TracingEvent {
            level: "INFO".into(),
            message: "started".into(),
        }
        .to_string(),
        "[INFO] started"
    );

    assert_eq!(
        AppEventKind::DashboardUpdated.to_string(),
        "dashboard updated"
    );

    assert_eq!(
        AppEventKind::SessionUpdated {
            session_key: key.clone(),
        }
        .to_string(),
        format!("session updated: {key}")
    );

    assert_eq!(
        AppEventKind::RunUpdated {
            team_run_id: "run-1".into(),
            status: "running".into(),
        }
        .to_string(),
        "run updated: run-1 (running)"
    );

    assert_eq!(
        AppEventKind::QueueUpdated {
            team_run_id: Some("run-1".into()),
        }
        .to_string(),
        "queue updated: run-1"
    );

    assert_eq!(
        AppEventKind::TeamRunStarted {
            team: "review".into(),
            workflow: "chain".into(),
            input: "check code".into(),
        }
        .to_string(),
        "team run started: review (chain)"
    );

    assert_eq!(
        AppEventKind::TeamStepStarted {
            team: "review".into(),
            agent: "coder".into(),
            step: 0,
        }
        .to_string(),
        "team review: step 0 started (agent: coder)"
    );

    assert_eq!(
        AppEventKind::TeamStepCompleted {
            team: "review".into(),
            agent: "coder".into(),
        }
        .to_string(),
        "team review: agent coder completed"
    );

    assert_eq!(
        AppEventKind::TeamStepFailed {
            team: "review".into(),
            agent: "coder".into(),
            reason: "crash".into(),
        }
        .to_string(),
        "team review: agent coder failed: crash"
    );

    assert_eq!(
        AppEventKind::TeamRunCompleted {
            team: "review".into(),
        }
        .to_string(),
        "team run completed: review"
    );

    assert_eq!(
        AppEventKind::TeamRunFailed {
            team: "review".into(),
            reason: "all failed".into(),
        }
        .to_string(),
        "team run failed: review: all failed"
    );

    assert_eq!(
        AppEventKind::ModelChanged {
            session_key: key.clone(),
            model: "claude-sonnet-4-6".into(),
            mode: "auto".into(),
        }
        .to_string(),
        "model changed: claude-sonnet-4-6 (auto)"
    );

    assert_eq!(
        AppEventKind::ContextCompacted {
            session_key: key.clone(),
        }
        .to_string(),
        format!("context compacted: {key}")
    );

    assert_eq!(
        AppEventKind::ExtensionNotification {
            session_key: key.clone(),
            extension: "developer".into(),
        }
        .to_string(),
        "extension notification: developer"
    );
}

#[test]
fn test_channel_reconnecting_display() {
    assert_eq!(
        AppEventKind::ChannelReconnecting {
            platform: Platform::Slack,
            attempt: 3,
            delay_secs: 5,
        }
        .to_string(),
        "slack reconnecting (attempt 3, delay 5s)"
    );

    assert_eq!(
        AppEventKind::ChannelReconnecting {
            platform: Platform::Discord,
            attempt: 1,
            delay_secs: 0,
        }
        .to_string(),
        "discord reconnecting (attempt 1, delay 0s)"
    );
}

#[test]
fn test_streaming_event_kind_display() {
    let key = SessionKey::new(Platform::Discord, "g1", "ch1");

    assert_eq!(
        AppEventKind::StreamStarted {
            session_key: key.clone(),
            stream_id: "s-42".into(),
        }
        .to_string(),
        "stream started: s-42"
    );

    assert_eq!(
        AppEventKind::StreamUpdated {
            session_key: key.clone(),
            stream_id: "s-42".into(),
            content_len: 128,
        }
        .to_string(),
        "stream updated: s-42 (128 bytes)"
    );

    assert_eq!(
        AppEventKind::StreamCompleted {
            session_key: key,
            stream_id: "s-42".into(),
            full_text: "hello world".into(),
        }
        .to_string(),
        "stream completed: s-42"
    );
}

#[test]
fn test_app_event_kind_serializes_with_type_tag() {
    let value = serde_json::to_value(AppEventKind::MessageReceived {
        session_key: SessionKey::from_stable_id("discord:ns:ops:bridge"),
        author: "alice".into(),
        content: "hello".into(),
    })
    .expect("event should serialize");

    assert_eq!(value["type"], "message_received");
    assert_eq!(value["session_key"], "discord:ns:ops:bridge");
}

#[test]
fn test_app_event_kind_display_additional_variants() {
    assert_eq!(
        AppEventKind::QueueUpdated { team_run_id: None }.to_string(),
        "queue updated"
    );

    assert_eq!(
        AppEventKind::AlertFired {
            rule_name: "high-latency".into(),
            metric: "latency_ms".into(),
            value: 250.0,
            platform: "discord".into(),
            channel_id: "ops-room".into(),
        }
        .to_string(),
        "alert fired: high-latency -> discord:ops-room"
    );

    assert_eq!(
        AppEventKind::ShutdownStarted {
            timeout_secs: 30,
            active_streams: 4,
        }
        .to_string(),
        "shutdown started: draining 4 stream(s), timeout 30s"
    );

    assert_eq!(
        AppEventKind::ShutdownCompleted {
            timed_out: false,
            remaining_streams: 0,
        }
        .to_string(),
        "shutdown completed: timed_out=false, remaining_streams=0"
    );
}

#[test]
fn test_app_event_kind_serde_roundtrips_all_variants() {
    for event in sample_app_event_kinds() {
        let value = serde_json::to_value(&event).expect("event should serialize");
        let parsed: AppEventKind =
            serde_json::from_value(value.clone()).expect("event should deserialize");
        let reparsed_value = serde_json::to_value(parsed).expect("event should reserialize");

        assert_eq!(reparsed_value, value);
    }
}

#[test]
fn test_app_event_kind_key_matches_serialized_type_tag() {
    for event in sample_app_event_kinds() {
        let value = serde_json::to_value(&event).expect("event should serialize");

        assert_eq!(
            value["type"].as_str(),
            Some(event.key()),
            "serialized type tag should match key()"
        );
    }
}

#[test]
fn test_app_event_kind_helper_accessors() {
    let key = sample_session_key();

    let channel_event = AppEventKind::ChannelDisconnected {
        platform: Platform::Slack,
        reason: "network timeout".into(),
    };
    assert_eq!(channel_event.source_gateway(), Some("slack"));
    assert!(channel_event.session_key().is_none());

    let session_event = AppEventKind::ResponseSent {
        session_key: key.clone(),
        content: "hello".into(),
    };
    assert_eq!(session_event.source_gateway(), Some("discord"));
    assert_eq!(
        session_event.session_key().map(SessionKey::to_stable_id),
        Some(key.to_stable_id())
    );

    let alert_event = AppEventKind::AlertFired {
        rule_name: "high-latency".into(),
        metric: "latency_ms".into(),
        value: 250.0,
        platform: "matrix".into(),
        channel_id: "ops-room".into(),
    };
    assert_eq!(alert_event.source_gateway(), Some("matrix"));
    assert!(alert_event.session_key().is_none());

    let system_event = AppEventKind::ShutdownCompleted {
        timed_out: false,
        remaining_streams: 0,
    };
    assert!(system_event.source_gateway().is_none());
    assert!(system_event.session_key().is_none());
}
