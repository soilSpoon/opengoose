mod modals;
mod scroll;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, AppMode, Panel};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Priority 1: Secret input modal (both modes)
    if app.secret_input.visible {
        modals::handle_secret_input_key(app, key);
        return;
    }

    // Priority 2: Provider selection modal
    if app.provider_select.visible {
        modals::handle_provider_select_key(app, key);
        return;
    }

    // Priority 3: Model selection modal
    if app.model_select.visible {
        modals::handle_model_select_key(app, key);
        return;
    }

    // Priority 4: Setup mode
    if app.mode == AppMode::Setup {
        modals::handle_setup_key(app, key);
        return;
    }

    // Priority 5: Command palette
    if app.command_palette.visible {
        modals::handle_command_palette_key(app, key);
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
        _ if app.active_panel == Panel::Messages => handle_messages_key(app, key),
        _ => handle_navigation_key(app, key),
    }
}

fn handle_messages_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.submit_composer(),
        KeyCode::Left => app.composer.move_left(),
        KeyCode::Right => app.composer.move_right(),
        KeyCode::Home => app.composer.move_home(),
        KeyCode::End => app.composer.move_end(),
        KeyCode::Backspace => app.composer.backspace(),
        KeyCode::Delete => app.composer.delete(),
        KeyCode::Up => app.composer.history_previous(),
        KeyCode::Down => app.composer.history_next(),
        KeyCode::PageUp => scroll::page_up(app),
        KeyCode::PageDown => scroll::page_down(app),
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            app.composer.insert_char(c);
        }
        _ => {}
    }
}

fn handle_navigation_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => scroll::scroll_down(app),
        KeyCode::Char('k') | KeyCode::Up => scroll::scroll_up(app),
        KeyCode::PageUp => scroll::page_up(app),
        KeyCode::PageDown => scroll::page_down(app),
        KeyCode::Char('G') => scroll::scroll_to_bottom(app),
        KeyCode::Char('g') => scroll::scroll_to_top(app),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
