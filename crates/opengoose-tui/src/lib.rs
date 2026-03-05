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

/// Process a single TUI event, returning `true` if the loop should exit.
fn process_event(app: &mut app::App, evt: event::TuiEvent) -> bool {
    match evt {
        event::TuiEvent::Key(key) => input::handle_key(app, key),
        event::TuiEvent::AppEvent(e) => app.handle_app_event(e),
        event::TuiEvent::Tick => app.tick(),
        event::TuiEvent::Resize => {}
        event::TuiEvent::Quit => return true,
    }
    app.should_quit
}

/// Run the TUI event loop with a generic backend.
async fn run_loop<B: Backend<Error: Send + Sync + 'static>>(
    terminal: &mut Terminal<B>,
    app: &mut app::App,
    events: &mut event::EventHandler,
) -> Result<()> {
    loop {
        if app.mode == app::AppMode::Normal {
            let size = terminal.size()?;
            let layout = ui::layout::create_layout(size.into());
            app.messages_area_height = layout.messages.height.saturating_sub(2) as usize;
            app.events_area_height = layout.events.height.saturating_sub(2) as usize;
        }

        terminal.draw(|f| ui::render_app(f, app))?;

        let evt = events.next().await;
        if process_event(app, evt) {
            break;
        }
    }
    Ok(())
}

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

    run_loop(&mut terminal, &mut app, &mut events).await?;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    cancel.cancel();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_types::{AppEvent, AppEventKind};
    use ratatui::backend::TestBackend;
    use std::time::Instant;

    fn test_app() -> app::App {
        app::App::new(app::AppMode::Normal, None, None)
    }

    #[test]
    fn test_process_event_tick() {
        let mut app = test_app();
        assert!(!process_event(&mut app, event::TuiEvent::Tick));
    }

    #[test]
    fn test_process_event_quit() {
        let mut app = test_app();
        assert!(process_event(&mut app, event::TuiEvent::Quit));
    }

    #[test]
    fn test_process_event_resize() {
        let mut app = test_app();
        assert!(!process_event(&mut app, event::TuiEvent::Resize));
    }

    #[test]
    fn test_process_event_app_event() {
        let mut app = test_app();
        let evt = event::TuiEvent::AppEvent(AppEvent {
            kind: AppEventKind::ChannelReady {
                platform: opengoose_types::Platform::Discord,
            },
            timestamp: Instant::now(),
        });
        assert!(!process_event(&mut app, evt));
        assert!(app.connected_platforms.contains(&opengoose_types::Platform::Discord));
    }

    #[test]
    fn test_process_event_key_quit() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = test_app();
        let key = KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        assert!(process_event(&mut app, event::TuiEvent::Key(key)));
    }

    #[tokio::test]
    async fn test_run_loop_quit() {
        let bus = EventBus::new(16);
        let cancel = CancellationToken::new();
        let mut app = test_app();
        let mut events = event::EventHandler::new(bus.subscribe(), cancel.clone());

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Cancel immediately so the handler sends Quit
        cancel.cancel();

        let result = run_loop(&mut terminal, &mut app, &mut events).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_loop_setup_mode() {
        let bus = EventBus::new(16);
        let cancel = CancellationToken::new();
        let mut app = app::App::new(app::AppMode::Setup, None, None);
        let mut events = event::EventHandler::new(bus.subscribe(), cancel.clone());

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        cancel.cancel();

        let result = run_loop(&mut terminal, &mut app, &mut events).await;
        assert!(result.is_ok());
    }
}
