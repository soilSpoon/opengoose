mod command_palette;
mod events_panel;
mod help_bar;
pub(crate) mod layout;
pub(crate) mod messages;
mod secret_input;
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
            messages::render(f, app, chunks.messages);
            events_panel::render(f, app, chunks.events);
            help_bar::render(f, app, chunks.help_bar);

            if app.command_palette.visible {
                command_palette::render(f, app);
            }
        }
    }

    // Secret input overlay renders on top of both modes
    if app.secret_input.visible {
        secret_input::render(f, app);
    }
}
