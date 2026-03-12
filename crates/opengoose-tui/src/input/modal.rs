use crossterm::event::{KeyCode, KeyEvent};

use crate::app::App;

pub(super) fn handle_secret_input_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.secret_input.visible = false;
            app.secret_input.input.clear();
            app.secret_input.status_message = None;
            app.secret_input.title = None;
            app.secret_input.is_secret = true;
            // Cancel any in-progress credential flow and clear sensitive data
            app.credential_flow.reset();
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

pub(super) fn handle_provider_select_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.provider_select.visible = false;
            app.credential_flow.reset();
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

pub(super) fn handle_model_select_key(app: &mut App, key: KeyEvent) {
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
