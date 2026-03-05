use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, Panel};
use crate::command;

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Priority 1: Secret input modal (both modes)
    if app.secret_input.visible {
        handle_secret_input_key(app, key);
        return;
    }

    // Priority 2: Provider selection modal
    if app.provider_select.visible {
        handle_provider_select_key(app, key);
        return;
    }

    // Priority 3: Model selection modal
    if app.model_select.visible {
        handle_model_select_key(app, key);
        return;
    }

    // Priority 4: Setup mode
    if app.mode == AppMode::Setup {
        handle_setup_key(app, key);
        return;
    }

    // Priority 5: Command palette
    if app.command_palette.visible {
        handle_command_palette_key(app, key);
        return;
    }

    // Priority 6: Normal mode
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.command_palette.visible = true;
            app.command_palette.input.clear();
            app.command_palette.selected = 0;
        }
        KeyCode::Tab => {
            app.active_panel = match app.active_panel {
                Panel::Messages => Panel::Events,
                Panel::Events => Panel::Messages,
            };
        }
        KeyCode::Char('j') | KeyCode::Down => scroll_down(app),
        KeyCode::Char('k') | KeyCode::Up => scroll_up(app),
        KeyCode::Char('G') => scroll_to_bottom(app),
        KeyCode::Char('g') => scroll_to_top(app),
        _ => {}
    }
}

fn handle_secret_input_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.secret_input.visible = false;
            app.secret_input.input.clear();
            app.secret_input.status_message = None;
            app.secret_input.title = None;
            app.secret_input.is_secret = true;
            // Also cancel any in-progress credential flow
            app.credential_flow.provider_id = None;
        }
        KeyCode::Enter => {
            if app.credential_flow.provider_id.is_some() {
                // Multi-step credential flow
                if let Err(e) = app.save_credential_and_advance() {
                    app.secret_input.status_message = Some(format!("Error: {e}"));
                }
            } else {
                // Original discord token flow
                if let Err(e) = app.save_secret_and_notify() {
                    app.secret_input.status_message = Some(format!("Error: {e}"));
                }
            }
        }
        KeyCode::Char(c) => {
            app.secret_input.input.push(c);
            app.secret_input.status_message = None;
        }
        KeyCode::Backspace => {
            app.secret_input.input.pop();
            app.secret_input.status_message = None;
        }
        _ => {}
    }
}

fn handle_provider_select_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.provider_select.visible = false;
        }
        KeyCode::Enter => {
            app.confirm_provider_select();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.provider_select.selected = app.provider_select.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.provider_select.providers.len().saturating_sub(1);
            if app.provider_select.selected < max {
                app.provider_select.selected += 1;
            }
        }
        _ => {}
    }
}

fn handle_model_select_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.model_select.visible = false;
        }
        KeyCode::Enter => {
            if let Some(model) = app.model_select.models.get(app.model_select.selected) {
                app.push_event(
                    &format!("Selected model: {model}"),
                    crate::app::EventLevel::Info,
                );
                app.model_select.visible = false;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.model_select.selected = app.model_select.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.model_select.models.len().saturating_sub(1);
            if app.model_select.selected < max {
                app.model_select.selected += 1;
            }
        }
        _ => {}
    }
}

fn handle_setup_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            app.secret_input.visible = true;
            app.secret_input.input.clear();
            app.secret_input.status_message = None;
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
        }
        _ => {}
    }
}

fn handle_command_palette_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.command_palette.visible = false;
        }
        KeyCode::Enter => {
            let commands = command::get_commands();
            let filtered = command::filter_commands(&commands, &app.command_palette.input);
            if let Some(cmd) = filtered.get(app.command_palette.selected) {
                let id = cmd.id;
                app.command_palette.visible = false;
                command::execute(app, id);
            }
        }
        KeyCode::Up => {
            app.command_palette.selected = app.command_palette.selected.saturating_sub(1);
        }
        KeyCode::Down => {
            let commands = command::get_commands();
            let filtered = command::filter_commands(&commands, &app.command_palette.input);
            let max = filtered.len().saturating_sub(1);
            if app.command_palette.selected < max {
                app.command_palette.selected += 1;
            }
        }
        KeyCode::Char(c) => {
            app.command_palette.input.push(c);
            app.command_palette.selected = 0;
        }
        KeyCode::Backspace => {
            app.command_palette.input.pop();
            app.command_palette.selected = 0;
        }
        _ => {}
    }
}

fn scroll_down(app: &mut App) {
    match app.active_panel {
        Panel::Messages => {
            let max = app
                .messages_line_count()
                .saturating_sub(app.messages_area_height);
            app.messages_scroll = app.messages_scroll.saturating_add(1).min(max);
        }
        Panel::Events => {
            let max = app
                .events_line_count()
                .saturating_sub(app.events_area_height);
            app.events_scroll = app.events_scroll.saturating_add(1).min(max);
        }
    }
}

fn scroll_up(app: &mut App) {
    match app.active_panel {
        Panel::Messages => app.messages_scroll = app.messages_scroll.saturating_sub(1),
        Panel::Events => app.events_scroll = app.events_scroll.saturating_sub(1),
    }
}

fn scroll_to_bottom(app: &mut App) {
    match app.active_panel {
        Panel::Messages => {
            app.messages_scroll = app
                .messages_line_count()
                .saturating_sub(app.messages_area_height);
        }
        Panel::Events => {
            app.events_scroll = app
                .events_line_count()
                .saturating_sub(app.events_area_height);
        }
    }
}

fn scroll_to_top(app: &mut App) {
    match app.active_panel {
        Panel::Messages => app.messages_scroll = 0,
        Panel::Events => app.events_scroll = 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::EventLevel;
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
    fn test_handle_key_quit() {
        let mut app = test_app();
        handle_key(&mut app, key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_key_tab_toggles_panel() {
        let mut app = test_app();
        assert_eq!(app.active_panel, Panel::Messages);
        handle_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.active_panel, Panel::Events);
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
        handle_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.messages_scroll, 0);
    }

    #[test]
    fn test_scroll_up_messages_panel() {
        let mut app = test_app();
        app.active_panel = Panel::Messages;
        app.messages_scroll = 5;
        handle_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.messages_scroll, 4);
    }

    #[test]
    fn test_scroll_to_top_messages() {
        let mut app = test_app();
        app.active_panel = Panel::Messages;
        app.messages_scroll = 10;
        handle_key(&mut app, key(KeyCode::Char('g')));
        assert_eq!(app.messages_scroll, 0);
    }

    #[test]
    fn test_scroll_to_bottom_messages() {
        let mut app = test_app();
        app.active_panel = Panel::Messages;
        app.messages_area_height = 10;
        // No messages, so scroll stays 0
        handle_key(&mut app, key(KeyCode::Char('G')));
        assert_eq!(app.messages_scroll, 0);
    }

    #[test]
    fn test_unknown_key_no_effect() {
        let mut app = test_app();
        handle_key(&mut app, key(KeyCode::F(12)));
        assert!(!app.should_quit);
        assert_eq!(app.active_panel, Panel::Messages);
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
            session_key: opengoose_types::SessionKey::dm("u"),
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
}
