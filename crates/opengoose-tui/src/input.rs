use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, Panel};
use crate::command;

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Priority 1: Secret input modal (both modes)
    if app.secret_input.visible {
        handle_secret_input_key(app, key);
        return;
    }

    // Priority 2: Setup mode
    if app.mode == AppMode::Setup {
        handle_setup_key(app, key);
        return;
    }

    // Priority 3: Command palette
    if app.command_palette.visible {
        handle_command_palette_key(app, key);
        return;
    }

    // Priority 4: Normal mode
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
        }
        KeyCode::Enter => {
            if let Err(e) = app.save_secret_and_notify() {
                app.secret_input.status_message = Some(format!("Error: {e}"));
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
}
