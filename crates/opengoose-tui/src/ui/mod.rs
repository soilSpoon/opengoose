//! TUI rendering: layout, panels, and widgets.
//!
//! Translates [`App`] state into Ratatui widgets drawn to the terminal.
//! Organised into focused sub-modules: `composer`, `events_panel`,
//! `command_palette`, `help_bar`, and `layout`.

mod command_palette;
mod composer;
mod events_panel;
mod help_bar;
pub(crate) mod layout;
pub(crate) mod messages;
mod model_select;
mod provider_select;
mod secret_input;
mod sessions;
mod setup_wizard;
mod status_bar;

use ratatui::Frame;

use crate::app::{App, AppMode};

pub fn render_app(f: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Setup => {
            setup_wizard::render(f);
        }
        AppMode::Normal => {
            let chunks = layout::create_layout(f.area());

            status_bar::render(f, app, chunks.status_bar);
            sessions::render(f, app, chunks.sessions);
            messages::render(f, app, chunks.messages);
            events_panel::render(f, app, chunks.events);
            composer::render(f, app, chunks.composer);
            help_bar::render(f, app, chunks.help_bar);

            if app.command_palette.visible {
                command_palette::render(f, app);
            }
        }
    }

    // Provider selection overlay
    if app.provider_select.visible {
        provider_select::render(f, app);
    }

    // Model selection overlay
    if app.model_select.visible {
        model_select::render(f, app);
    }

    // Secret input overlay renders on top of both modes
    if app.secret_input.visible {
        secret_input::render(f, app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_app_normal_mode() {
        let app = App::new(AppMode::Normal, None, None);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_app(f, &app)).unwrap();
    }

    #[test]
    fn test_render_app_setup_mode() {
        let app = App::new(AppMode::Setup, None, None);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_app(f, &app)).unwrap();
    }

    #[test]
    fn test_render_app_with_command_palette() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.command_palette.visible = true;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_app(f, &app)).unwrap();
    }

    #[test]
    fn test_render_app_with_secret_input_normal() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.secret_input.visible = true;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_app(f, &app)).unwrap();
    }

    #[test]
    fn test_render_app_with_secret_input_setup() {
        let mut app = App::new(AppMode::Setup, None, None);
        app.secret_input.visible = true;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_app(f, &app)).unwrap();
    }
}
