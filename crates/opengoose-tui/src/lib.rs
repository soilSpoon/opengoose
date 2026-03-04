mod app;
mod command;
mod event;
mod input;
mod theme;
mod tracing_layer;
mod ui;

pub use app::AppMode;
pub use tracing_layer::TuiTracingLayer;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use opengoose_types::EventBus;
use ratatui::prelude::*;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub async fn run_tui(
    event_bus: EventBus,
    cancel: CancellationToken,
    mode: AppMode,
    token_sender: Option<oneshot::Sender<String>>,
    pairing_tx: Option<mpsc::UnboundedSender<()>>,
) -> Result<()> {
    // Install panic hook that restores terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stderr(), LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = app::App::new(mode, token_sender, pairing_tx);
    let mut events = event::EventHandler::new(event_bus.subscribe(), cancel.clone());

    loop {
        // Compute panel viewport heights before drawing (for scroll clamping)
        if app.mode == app::AppMode::Normal {
            let size = terminal.size()?;
            let layout = ui::layout::create_layout(size.into());
            app.messages_area_height = layout.messages.height.saturating_sub(2);
            app.events_area_height = layout.events.height.saturating_sub(2);
        }

        terminal.draw(|f| ui::render_app(f, &app))?;

        match events.next().await {
            event::TuiEvent::Key(key) => input::handle_key(&mut app, key),
            event::TuiEvent::AppEvent(e) => app.handle_app_event(e),
            event::TuiEvent::Tick => app.tick(),
            event::TuiEvent::Resize => {}
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    cancel.cancel();
    Ok(())
}
