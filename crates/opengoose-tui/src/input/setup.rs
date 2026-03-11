use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;

pub(super) fn handle_setup_key(app: &mut App, key: KeyEvent) {
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
