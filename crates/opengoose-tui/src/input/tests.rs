use super::*;
use crate::app::{AppMode, EventLevel, Panel};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn test_app() -> App {
    App::new(AppMode::Normal, None, None)
}

fn setup_app() -> App {
    App::new(AppMode::Setup, None, None)
}

// ── Normal mode tests ──────────────────────────────

#[test]
fn test_handle_key_ctrl_quit() {
    let mut app = test_app();
    let ctrl_q = KeyEvent {
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    handle_key(&mut app, ctrl_q);
    assert!(app.should_quit);
}

#[test]
fn test_handle_key_tab_toggles_panel() {
    let mut app = test_app();
    assert_eq!(app.active_panel, Panel::Messages);
    handle_key(&mut app, key(KeyCode::Tab));
    assert_eq!(app.active_panel, Panel::Events);
    handle_key(&mut app, key(KeyCode::Tab));
    assert_eq!(app.active_panel, Panel::Sessions);
    handle_key(&mut app, key(KeyCode::Tab));
    assert_eq!(app.active_panel, Panel::Messages);
}

#[test]
fn test_handle_key_command_palette() {
    let mut app = test_app();
    let ctrl_o = KeyEvent {
        code: KeyCode::Char('o'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    handle_key(&mut app, ctrl_o);
    assert!(app.command_palette.visible);
}

#[test]
fn test_scroll_down_messages() {
    let mut app = test_app();
    // Add events to make content taller than area
    for _ in 0..20 {
        app.push_event("e", EventLevel::Info);
    }
    app.active_panel = Panel::Events;
    app.events_area_height = 5;
    handle_key(&mut app, key(KeyCode::Char('j')));
    assert_eq!(app.events_scroll, 1);
}

#[test]
fn test_scroll_up_messages() {
    let mut app = test_app();
    app.active_panel = Panel::Events;
    app.events_scroll = 3;
    handle_key(&mut app, key(KeyCode::Char('k')));
    assert_eq!(app.events_scroll, 2);
}

#[test]
fn test_scroll_up_at_zero() {
    let mut app = test_app();
    app.events_scroll = 0;
    app.active_panel = Panel::Events;
    handle_key(&mut app, key(KeyCode::Up));
    assert_eq!(app.events_scroll, 0);
}

#[test]
fn test_scroll_to_top() {
    let mut app = test_app();
    app.events_scroll = 10;
    app.active_panel = Panel::Events;
    handle_key(&mut app, key(KeyCode::Char('g')));
    assert_eq!(app.events_scroll, 0);
}

#[test]
fn test_scroll_to_bottom() {
    let mut app = test_app();
    for _ in 0..30 {
        app.push_event("e", EventLevel::Info);
    }
    app.active_panel = Panel::Events;
    app.events_area_height = 10;
    handle_key(&mut app, key(KeyCode::Char('G')));
    assert_eq!(app.events_scroll, 20); // 30 - 10
}

#[test]
fn test_scroll_down_messages_panel() {
    let mut app = test_app();
    app.active_panel = Panel::Messages;
    app.messages_scroll = 0;
    app.messages_area_height = 100; // Large area, nothing to scroll
    handle_key(&mut app, key(KeyCode::PageDown));
    assert_eq!(app.messages_scroll, 0);
}

#[test]
fn test_up_down_navigate_composer_history() {
    let mut app = test_app();
    app.active_panel = Panel::Messages;
    app.composer.push_history("older".into());
    app.composer.push_history("latest".into());
    handle_key(&mut app, key(KeyCode::Up));
    assert_eq!(app.composer.input, "latest");
    handle_key(&mut app, key(KeyCode::Up));
    assert_eq!(app.composer.input, "older");
    handle_key(&mut app, key(KeyCode::Down));
    assert_eq!(app.composer.input, "latest");
}

#[test]
fn test_scroll_to_top_messages() {
    let mut app = test_app();
    app.active_panel = Panel::Messages;
    app.messages_area_height = 10;
    app.messages_scroll = 10;
    handle_key(&mut app, key(KeyCode::PageUp));
    assert_eq!(app.messages_scroll, 1);
}

#[test]
fn test_scroll_to_bottom_messages() {
    let mut app = test_app();
    app.active_panel = Panel::Messages;
    app.messages_area_height = 3;
    app.messages_area_width = 20;
    for content in [
        "one two three four five six seven eight nine ten",
        "eleven twelve thirteen fourteen fifteen",
        "sixteen seventeen eighteen nineteen twenty",
    ] {
        app.messages.push_back(crate::app::MessageEntry {
            session_key: opengoose_types::SessionKey::dm(
                opengoose_types::Platform::Discord,
                "u",
            ),
            author: "a".into(),
            content: content.into(),
        });
    }
    handle_key(&mut app, key(KeyCode::PageDown));
    assert!(app.messages_scroll > 0);
}

#[test]
fn test_unknown_key_no_effect() {
    let mut app = test_app();
    handle_key(&mut app, key(KeyCode::F(12)));
    assert!(!app.should_quit);
    assert_eq!(app.active_panel, Panel::Messages);
}

#[test]
fn test_messages_panel_typing_edits_composer() {
    let mut app = test_app();
    handle_key(&mut app, key(KeyCode::Char('h')));
    handle_key(&mut app, key(KeyCode::Char('i')));
    assert_eq!(app.composer.input, "hi");
    assert_eq!(app.composer.cursor, 2);
}

#[test]
fn test_messages_panel_q_does_not_quit() {
    let mut app = test_app();
    handle_key(&mut app, key(KeyCode::Char('q')));
    assert!(!app.should_quit);
    assert_eq!(app.composer.input, "q");
}

#[test]
fn test_messages_panel_cursor_movement_and_backspace() {
    let mut app = test_app();
    app.composer.input = "helo".into();
    app.composer.cursor = 4;
    handle_key(&mut app, key(KeyCode::Left));
    handle_key(&mut app, key(KeyCode::Char('l')));
    handle_key(&mut app, key(KeyCode::Backspace));
    assert_eq!(app.composer.input, "helo");
    assert_eq!(app.composer.cursor, 3);
}

#[test]
fn test_messages_panel_home_end_move_cursor() {
    let mut app = test_app();
    app.composer.input = "hello".into();
    app.composer.cursor = 2;

    handle_key(&mut app, key(KeyCode::End));
    assert_eq!(app.composer.cursor, 5);

    handle_key(&mut app, key(KeyCode::Home));
    assert_eq!(app.composer.cursor, 0);
}

#[test]
fn test_messages_panel_enter_submits_composer() {
    let mut app = test_app();
    let session_key =
        opengoose_types::SessionKey::dm(opengoose_types::Platform::Discord, "user-1");
    app.sessions.push(crate::app::SessionListEntry {
        session_key: session_key.clone(),
        active_team: None,
        created_at: None,
        updated_at: None,
        is_active: true,
    });
    app.select_session(0);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.set_composer_tx(tx);
    app.composer.input = "hello".into();
    app.composer.cursor = 5;

    handle_key(&mut app, key(KeyCode::Enter));

    let request = rx.try_recv().unwrap();
    assert_eq!(request.session_key, session_key);
    assert_eq!(request.content, "hello");
    assert!(app.composer.input.is_empty());
}

// ── Setup mode tests ───────────────────────────────

#[test]
fn test_setup_enter_opens_secret_input() {
    let mut app = setup_app();
    handle_key(&mut app, key(KeyCode::Enter));
    assert!(app.secret_input.visible);
    assert!(app.secret_input.input.is_empty());
}

#[test]
fn test_setup_q_quits() {
    let mut app = setup_app();
    handle_key(&mut app, key(KeyCode::Char('q')));
    assert!(app.should_quit);
}

#[test]
fn test_setup_esc_quits() {
    let mut app = setup_app();
    handle_key(&mut app, key(KeyCode::Esc));
    assert!(app.should_quit);
}

#[test]
fn test_setup_unknown_key_no_effect() {
    let mut app = setup_app();
    handle_key(&mut app, key(KeyCode::Tab));
    assert!(!app.should_quit);
    assert!(!app.secret_input.visible);
}

// ── Secret input modal tests ───────────────────────

#[test]
fn test_secret_input_esc_closes() {
    let mut app = test_app();
    app.secret_input.visible = true;
    app.secret_input.input = "some_token".into();
    handle_key(&mut app, key(KeyCode::Esc));
    assert!(!app.secret_input.visible);
    assert!(app.secret_input.input.is_empty());
    assert!(app.secret_input.status_message.is_none());
}

#[test]
fn test_secret_input_typing() {
    let mut app = test_app();
    app.secret_input.visible = true;
    handle_key(&mut app, key(KeyCode::Char('a')));
    handle_key(&mut app, key(KeyCode::Char('b')));
    handle_key(&mut app, key(KeyCode::Char('c')));
    assert_eq!(app.secret_input.input, "abc");
}

#[test]
fn test_secret_input_backspace() {
    let mut app = test_app();
    app.secret_input.visible = true;
    app.secret_input.input = "abc".into();
    handle_key(&mut app, key(KeyCode::Backspace));
    assert_eq!(app.secret_input.input, "ab");
}

#[test]
fn test_secret_input_clears_status_on_type() {
    let mut app = test_app();
    app.secret_input.visible = true;
    app.secret_input.status_message = Some("error".into());
    handle_key(&mut app, key(KeyCode::Char('x')));
    assert!(app.secret_input.status_message.is_none());
}

#[test]
fn test_secret_input_clears_status_on_backspace() {
    let mut app = test_app();
    app.secret_input.visible = true;
    app.secret_input.input = "a".into();
    app.secret_input.status_message = Some("error".into());
    handle_key(&mut app, key(KeyCode::Backspace));
    assert!(app.secret_input.status_message.is_none());
}

#[test]
fn test_secret_input_blocks_normal_keys() {
    let mut app = test_app();
    app.secret_input.visible = true;
    // 'q' should NOT quit when secret input is visible
    handle_key(&mut app, key(KeyCode::Char('q')));
    assert!(!app.should_quit);
    assert_eq!(app.secret_input.input, "q");
}

#[test]
fn test_secret_input_unknown_key_no_effect() {
    let mut app = test_app();
    app.secret_input.visible = true;
    handle_key(&mut app, key(KeyCode::F(1)));
    assert!(app.secret_input.visible);
}

// ── Command palette tests ──────────────────────────

#[test]
fn test_command_palette_esc_closes() {
    let mut app = test_app();
    app.command_palette.visible = true;
    handle_key(&mut app, key(KeyCode::Esc));
    assert!(!app.command_palette.visible);
}

#[test]
fn test_command_palette_typing() {
    let mut app = test_app();
    app.command_palette.visible = true;
    handle_key(&mut app, key(KeyCode::Char('q')));
    handle_key(&mut app, key(KeyCode::Char('u')));
    assert_eq!(app.command_palette.input, "qu");
    assert_eq!(app.command_palette.selected, 0); // resets on type
}

#[test]
fn test_command_palette_backspace() {
    let mut app = test_app();
    app.command_palette.visible = true;
    app.command_palette.input = "abc".into();
    handle_key(&mut app, key(KeyCode::Backspace));
    assert_eq!(app.command_palette.input, "ab");
    assert_eq!(app.command_palette.selected, 0); // resets on backspace
}

#[test]
fn test_command_palette_up_down() {
    let mut app = test_app();
    app.command_palette.visible = true;
    // Start at 0, go down
    handle_key(&mut app, key(KeyCode::Down));
    assert_eq!(app.command_palette.selected, 1);
    handle_key(&mut app, key(KeyCode::Down));
    assert_eq!(app.command_palette.selected, 2);
    // Go back up
    handle_key(&mut app, key(KeyCode::Up));
    assert_eq!(app.command_palette.selected, 1);
    // Up at 0 stays at 0
    handle_key(&mut app, key(KeyCode::Up));
    assert_eq!(app.command_palette.selected, 0);
    handle_key(&mut app, key(KeyCode::Up));
    assert_eq!(app.command_palette.selected, 0);
}

#[test]
fn test_command_palette_enter_executes_quit() {
    let mut app = test_app();
    app.command_palette.visible = true;
    // Type "quit" to filter to Quit command
    for c in "quit".chars() {
        handle_key(&mut app, key(KeyCode::Char(c)));
    }
    // Enter should execute the selected (first filtered) command
    handle_key(&mut app, key(KeyCode::Enter));
    assert!(!app.command_palette.visible);
    assert!(app.should_quit);
}

#[test]
fn test_command_palette_enter_executes_clear_messages() {
    let mut app = test_app();
    // Add a message first
    app.messages.push_back(crate::app::MessageEntry {
        session_key: opengoose_types::SessionKey::dm(opengoose_types::Platform::Discord, "u"),
        author: "a".into(),
        content: "c".into(),
    });
    app.command_palette.visible = true;
    // Type "clear m" to get Clear Messages
    for c in "clear m".chars() {
        handle_key(&mut app, key(KeyCode::Char(c)));
    }
    handle_key(&mut app, key(KeyCode::Enter));
    assert!(app.messages.is_empty());
}

#[test]
fn test_command_palette_blocks_normal_keys() {
    let mut app = test_app();
    app.command_palette.visible = true;
    // 'q' should type into palette, not quit
    handle_key(&mut app, key(KeyCode::Char('q')));
    assert!(!app.should_quit);
    assert_eq!(app.command_palette.input, "q");
}

#[test]
fn test_command_palette_unknown_key_no_effect() {
    let mut app = test_app();
    app.command_palette.visible = true;
    handle_key(&mut app, key(KeyCode::F(5)));
    assert!(app.command_palette.visible);
}

#[test]
fn test_command_palette_down_respects_max() {
    let mut app = test_app();
    app.command_palette.visible = true;
    // 9 commands, max index = 8
    for _ in 0..12 {
        handle_key(&mut app, key(KeyCode::Down));
    }
    assert_eq!(app.command_palette.selected, 8);
}

#[test]
fn test_secret_input_enter_error_shows_status() {
    use opengoose_secrets::{SecretResult, SecretStore, SecretValue};
    use std::sync::Arc;

    // A store that always fails on set
    struct FailStore;
    impl SecretStore for FailStore {
        fn get(&self, _: &str) -> SecretResult<Option<SecretValue>> {
            Ok(None)
        }
        fn set(&self, _: &str, _: &str) -> SecretResult<()> {
            Err(opengoose_secrets::SecretError::NoHomeDir)
        }
        fn delete(&self, _: &str) -> SecretResult<bool> {
            Ok(false)
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let mut app = App::with_store(
        AppMode::Normal,
        None,
        None,
        Arc::new(FailStore),
        Some(config_path),
    );
    app.secret_input.visible = true;
    app.secret_input.input = "some_token".into();
    handle_key(&mut app, key(KeyCode::Enter));

    // Should show error status message
    assert!(app.secret_input.status_message.is_some());
    assert!(
        app.secret_input
            .status_message
            .as_ref()
            .unwrap()
            .contains("Error")
    );
}
