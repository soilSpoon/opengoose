mod command_palette;
mod modal;
mod navigation;
mod setup;

#[cfg(test)]
mod tests;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, Panel};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Priority 1: Secret input modal (both modes)
    if app.secret_input.visible {
        modal::handle_secret_input_key(app, key);
        return;
    }

    // Priority 2: Provider selection modal
    if app.provider_select.visible {
        modal::handle_provider_select_key(app, key);
        return;
    }

    // Priority 3: Model selection modal
    if app.model_select.visible {
        modal::handle_model_select_key(app, key);
        return;
    }

    // Priority 4: Setup mode
    if app.mode == AppMode::Setup {
        setup::handle_setup_key(app, key);
        return;
    }

    // Priority 5: Command palette
    if app.command_palette.visible {
        command_palette::handle_command_palette_key(app, key);
        return;
    }

    // Priority 6: Normal mode
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('n') => app.request_new_session(),
            KeyCode::Char('o') => {
                app.command_palette.visible = true;
                app.command_palette.input.clear();
                app.command_palette.selected = 0;
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.active_panel = match app.active_panel {
                Panel::Sessions => Panel::Events,
                Panel::Messages => Panel::Sessions,
                Panel::Events => Panel::Messages,
            };
        }
        KeyCode::Tab => {
            app.active_panel = match app.active_panel {
                Panel::Sessions => Panel::Messages,
                Panel::Messages => Panel::Events,
                Panel::Events => Panel::Sessions,
            };
        }
        _ if app.active_panel == Panel::Messages => navigation::handle_messages_key(app, key),
        _ => navigation::handle_navigation_key(app, key),
    }
}
