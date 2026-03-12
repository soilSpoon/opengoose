use super::super::handlers::truncate;
use super::super::*;

#[test]
fn test_trigger_type_roundtrip() {
    for name in TriggerType::all_names() {
        let tt = TriggerType::parse(name).unwrap();
        assert_eq!(tt.as_str(), *name);
    }
}

#[test]
fn test_trigger_type_invalid() {
    assert!(TriggerType::parse("bogus").is_none());
}

#[test]
fn test_validate_trigger_type() {
    assert!(validate_trigger_type("file_watch").is_ok());
    assert!(validate_trigger_type("message_received").is_ok());
    assert!(validate_trigger_type("webhook_received").is_ok());
    assert!(validate_trigger_type("schedule_complete").is_ok());
    assert!(validate_trigger_type("on_message").is_ok());
    assert!(validate_trigger_type("on_session_start").is_ok());
    assert!(validate_trigger_type("on_session_end").is_ok());
    assert!(validate_trigger_type("on_schedule").is_ok());
    assert!(validate_trigger_type("nope").is_err());
}

#[test]
fn test_validate_trigger_type_error_message_includes_valid_types() {
    let err = validate_trigger_type("invalid").unwrap_err();
    assert!(err.contains("file_watch"), "error should list valid types");
    assert!(
        err.contains("message_received"),
        "error should list valid types"
    );
    assert!(err.contains("invalid"), "error should mention the bad type");
}

#[test]
fn test_trigger_type_all_names_complete() {
    let names = TriggerType::all_names();
    assert_eq!(names.len(), 8);
    for name in names {
        assert!(TriggerType::parse(name).is_some());
    }
}

#[test]
fn test_trigger_type_parse_empty_string() {
    assert!(TriggerType::parse("").is_none());
}

#[test]
fn test_truncate_short_string() {
    assert_eq!(truncate("hello", 10), "hello");
}

#[test]
fn test_truncate_exact_boundary() {
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn test_truncate_long_string() {
    assert_eq!(truncate("hello world", 5), "hello");
}

#[test]
fn test_truncate_utf8_safety() {
    // 3-byte UTF-8 char: should truncate at valid char boundary
    let text = "aaa\u{2603}bbb"; // snowman (3 bytes)
    let result = truncate(text, 4);
    assert_eq!(result, "aaa"); // can't fit the snowman in 4 bytes
}

#[test]
fn test_truncate_empty_string() {
    assert_eq!(truncate("", 10), "");
    assert_eq!(truncate("", 0), "");
}
