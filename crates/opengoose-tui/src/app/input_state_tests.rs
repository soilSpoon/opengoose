use super::input_state::*;

// ── ComposerState basic editing ────────────────────────────────

#[test]
fn test_composer_new_defaults() {
    let c = ComposerState::new();
    assert!(c.input.is_empty());
    assert_eq!(c.cursor, 0);
    assert!(c.history.is_empty());
    assert!(c.history_index.is_none());
    assert!(c.history_draft.is_none());
}

#[test]
fn test_insert_char_appends_at_cursor() {
    let mut c = ComposerState::new();
    c.insert_char('a');
    c.insert_char('b');
    assert_eq!(c.input, "ab");
    assert_eq!(c.cursor, 2);
}

#[test]
fn test_insert_char_in_middle() {
    let mut c = ComposerState::new();
    c.input = "ac".into();
    c.cursor = 1;
    c.insert_char('b');
    assert_eq!(c.input, "abc");
    assert_eq!(c.cursor, 2);
}

#[test]
fn test_insert_multibyte_char() {
    let mut c = ComposerState::new();
    c.insert_char('é');
    c.insert_char('!');
    assert_eq!(c.input, "é!");
    assert_eq!(c.cursor, 2);
}

#[test]
fn test_backspace_at_zero_is_noop() {
    let mut c = ComposerState::new();
    c.input = "hello".into();
    c.cursor = 0;
    c.backspace();
    assert_eq!(c.input, "hello");
    assert_eq!(c.cursor, 0);
}

#[test]
fn test_backspace_removes_previous_char() {
    let mut c = ComposerState::new();
    c.input = "abc".into();
    c.cursor = 2;
    c.backspace();
    assert_eq!(c.input, "ac");
    assert_eq!(c.cursor, 1);
}

#[test]
fn test_backspace_multibyte() {
    let mut c = ComposerState::new();
    c.input = "aé".into();
    c.cursor = 2;
    c.backspace();
    assert_eq!(c.input, "a");
    assert_eq!(c.cursor, 1);
}

#[test]
fn test_delete_at_end_is_noop() {
    let mut c = ComposerState::new();
    c.input = "abc".into();
    c.cursor = 3;
    c.delete();
    assert_eq!(c.input, "abc");
}

#[test]
fn test_delete_removes_char_at_cursor() {
    let mut c = ComposerState::new();
    c.input = "abc".into();
    c.cursor = 1;
    c.delete();
    assert_eq!(c.input, "ac");
    assert_eq!(c.cursor, 1);
}

// ── Cursor movement ────────────────────────────────────────────

#[test]
fn test_move_left_at_zero() {
    let mut c = ComposerState::new();
    c.cursor = 0;
    c.move_left();
    assert_eq!(c.cursor, 0);
}

#[test]
fn test_move_left_decrements() {
    let mut c = ComposerState::new();
    c.input = "abc".into();
    c.cursor = 2;
    c.move_left();
    assert_eq!(c.cursor, 1);
}

#[test]
fn test_move_right_at_end() {
    let mut c = ComposerState::new();
    c.input = "abc".into();
    c.cursor = 3;
    c.move_right();
    assert_eq!(c.cursor, 3);
}

#[test]
fn test_move_right_increments() {
    let mut c = ComposerState::new();
    c.input = "abc".into();
    c.cursor = 0;
    c.move_right();
    assert_eq!(c.cursor, 1);
}

#[test]
fn test_move_home() {
    let mut c = ComposerState::new();
    c.input = "abc".into();
    c.cursor = 2;
    c.move_home();
    assert_eq!(c.cursor, 0);
}

#[test]
fn test_move_end() {
    let mut c = ComposerState::new();
    c.input = "abc".into();
    c.cursor = 0;
    c.move_end();
    assert_eq!(c.cursor, 3);
}

// ── History ────────────────────────────────────────────────────

#[test]
fn test_push_history_ignores_empty() {
    let mut c = ComposerState::new();
    c.push_history(String::new());
    assert!(c.history.is_empty());
}

#[test]
fn test_push_history_deduplicates() {
    let mut c = ComposerState::new();
    c.push_history("alpha".into());
    c.push_history("beta".into());
    c.push_history("alpha".into());
    assert_eq!(c.history.len(), 2);
    assert_eq!(c.history[0], "beta");
    assert_eq!(c.history[1], "alpha");
}

#[test]
fn test_push_history_enforces_limit() {
    let mut c = ComposerState::new();
    for i in 0..60 {
        c.push_history(format!("entry {i}"));
    }
    assert!(c.history.len() <= 50);
    // Newest entry is preserved
    assert_eq!(c.history.back().unwrap(), "entry 59");
}

#[test]
fn test_history_previous_empty_is_noop() {
    let mut c = ComposerState::new();
    c.input = "current".into();
    c.history_previous();
    assert_eq!(c.input, "current");
    assert!(c.history_index.is_none());
}

#[test]
fn test_history_navigation_round_trip() {
    let mut c = ComposerState::new();
    c.push_history("first".into());
    c.push_history("second".into());
    c.input = "draft".into();
    c.cursor = 5;

    // Navigate to most recent
    c.history_previous();
    assert_eq!(c.input, "second");
    assert_eq!(c.history_index, Some(1));
    assert_eq!(c.cursor, 6); // cursor at end

    // Navigate to oldest
    c.history_previous();
    assert_eq!(c.input, "first");
    assert_eq!(c.history_index, Some(0));

    // At oldest, previous is noop
    c.history_previous();
    assert_eq!(c.input, "first");
    assert_eq!(c.history_index, Some(0));

    // Navigate forward
    c.history_next();
    assert_eq!(c.input, "second");
    assert_eq!(c.history_index, Some(1));

    // Navigate past newest restores draft
    c.history_next();
    assert_eq!(c.input, "draft");
    assert!(c.history_index.is_none());
}

#[test]
fn test_history_next_without_navigation_is_noop() {
    let mut c = ComposerState::new();
    c.push_history("entry".into());
    c.input = "current".into();
    c.history_next();
    assert_eq!(c.input, "current");
}

#[test]
fn test_insert_char_clears_history_navigation() {
    let mut c = ComposerState::new();
    c.push_history("old".into());
    c.history_previous();
    assert!(c.history_index.is_some());

    c.insert_char('x');
    assert!(c.history_index.is_none());
    assert!(c.history_draft.is_none());
}

#[test]
fn test_backspace_clears_history_navigation() {
    let mut c = ComposerState::new();
    c.push_history("old".into());
    c.history_previous();
    assert!(c.history_index.is_some());

    c.backspace();
    assert!(c.history_index.is_none());
    assert!(c.history_draft.is_none());
}

#[test]
fn test_delete_clears_history_navigation() {
    let mut c = ComposerState::new();
    c.push_history("old".into());
    c.history_previous();
    c.cursor = 0;

    c.delete();
    assert!(c.history_index.is_none());
    assert!(c.history_draft.is_none());
}

#[test]
fn test_clear_resets_all() {
    let mut c = ComposerState::new();
    c.input = "something".into();
    c.cursor = 5;
    c.push_history("entry".into());
    c.history_previous();

    c.clear();

    assert!(c.input.is_empty());
    assert_eq!(c.cursor, 0);
    assert!(c.history_index.is_none());
    assert!(c.history_draft.is_none());
    // history itself is preserved
    assert_eq!(c.history.len(), 1);
}

// ── CredentialFlowState ────────────────────────────────────────

#[test]
fn test_credential_flow_current_advances() {
    let mut cf = CredentialFlowState::new();
    let key1 = CredentialKey {
        env_var: "A".into(),
        label: "a".into(),
        secret: false,
        oauth_flow: false,
        required: true,
        default: None,
    };
    let key2 = CredentialKey {
        env_var: "B".into(),
        label: "b".into(),
        secret: true,
        oauth_flow: false,
        required: false,
        default: Some("default_val".into()),
    };
    cf.keys.push(key1);
    cf.keys.push(key2);

    assert_eq!(cf.current().unwrap().env_var, "A");
    assert!(cf.has_more());

    cf.current_key = 1;
    assert_eq!(cf.current().unwrap().env_var, "B");
    assert!(!cf.has_more());

    cf.current_key = 2;
    assert!(cf.current().is_none());
}

// ── ProviderSelectPurpose ──────────────────────────────────────

#[test]
fn test_provider_select_purpose_equality() {
    assert_eq!(
        ProviderSelectPurpose::Configure,
        ProviderSelectPurpose::Configure
    );
    assert_ne!(
        ProviderSelectPurpose::Configure,
        ProviderSelectPurpose::ListModels
    );
}
