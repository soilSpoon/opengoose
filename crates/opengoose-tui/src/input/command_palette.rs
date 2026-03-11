use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;
use crate::command;

pub(super) fn handle_command_palette_key(app: &mut App, key: KeyEvent) {
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
