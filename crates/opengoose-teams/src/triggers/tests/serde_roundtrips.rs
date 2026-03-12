use super::super::*;

#[test]
fn test_message_condition_deserialize_default() {
    let cond: MessageCondition = serde_json::from_str("{}").unwrap();
    assert!(cond.from_agent.is_none());
    assert!(cond.channel.is_none());
    assert!(cond.payload_contains.is_none());
}

#[test]
fn test_message_condition_serialize_skips_none() {
    let cond = MessageCondition {
        from_agent: Some("agent-a".into()),
        channel: None,
        payload_contains: None,
    };
    let json = serde_json::to_string(&cond).unwrap();
    assert!(json.contains("from_agent"));
    assert!(!json.contains("channel"));
    assert!(!json.contains("payload_contains"));
}

#[test]
fn test_file_watch_condition_roundtrip() {
    let cond = FileWatchCondition {
        pattern: Some("src/**/*.rs".into()),
    };
    let json = serde_json::to_string(&cond).unwrap();
    let parsed: FileWatchCondition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.pattern, Some("src/**/*.rs".into()));
}

#[test]
fn test_webhook_condition_roundtrip() {
    let cond = WebhookCondition {
        path: Some("/github/pr".into()),
        secret: None,
        hmac_secret: Some("signing-secret".into()),
        signature_header: Some("X-Hub-Signature-256".into()),
        timestamp_header: Some("X-Hub-Timestamp".into()),
        timestamp_tolerance_secs: Some(120),
    };
    let json = serde_json::to_string(&cond).unwrap();
    let parsed: WebhookCondition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.path, Some("/github/pr".into()));
    assert_eq!(parsed.hmac_secret, Some("signing-secret".into()));
    assert_eq!(parsed.signature_header, Some("X-Hub-Signature-256".into()));
    assert_eq!(parsed.timestamp_header, Some("X-Hub-Timestamp".into()));
    assert_eq!(parsed.timestamp_tolerance_secs, Some(120));
}

#[test]
fn test_schedule_complete_condition_roundtrip() {
    let cond = ScheduleCompleteCondition {
        schedule_name: Some("nightly-build".into()),
    };
    let json = serde_json::to_string(&cond).unwrap();
    let parsed: ScheduleCompleteCondition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.schedule_name, Some("nightly-build".into()));
}

#[test]
fn test_trigger_type_serde_roundtrip() {
    for tt in [
        TriggerType::FileWatch,
        TriggerType::MessageReceived,
        TriggerType::ScheduleComplete,
        TriggerType::WebhookReceived,
        TriggerType::OnMessage,
        TriggerType::OnSessionStart,
        TriggerType::OnSessionEnd,
        TriggerType::OnSchedule,
    ] {
        let json = serde_json::to_string(&tt).unwrap();
        let parsed: TriggerType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tt);
    }
}

#[test]
fn test_on_message_condition_serde_roundtrip() {
    let cond = OnMessageCondition {
        from_author: Some("alice".into()),
        content_contains: Some("hello".into()),
    };
    let json = serde_json::to_string(&cond).unwrap();
    let parsed: OnMessageCondition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.from_author, Some("alice".into()));
    assert_eq!(parsed.content_contains, Some("hello".into()));
}

#[test]
fn test_on_session_condition_serde_roundtrip() {
    let cond = OnSessionCondition {
        platform: Some("discord".into()),
    };
    let json = serde_json::to_string(&cond).unwrap();
    let parsed: OnSessionCondition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.platform, Some("discord".into()));
}

#[test]
fn test_on_schedule_condition_serde_roundtrip() {
    let cond = OnScheduleCondition {
        team: Some("nightly-review".into()),
    };
    let json = serde_json::to_string(&cond).unwrap();
    let parsed: OnScheduleCondition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.team, Some("nightly-review".into()));
}
