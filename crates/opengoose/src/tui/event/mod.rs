mod commands;
mod key_command;
mod keys;
mod rigs;

use anyhow::Result;
use crossterm::{
    event::{Event, EventStream},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use opengoose_board::Board;
use opengoose_rig::rig::Operator;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use super::app::App;
use super::log_entry::LogEntry;
use super::ui;
use keys::handle_key;
use rigs::load_rigs;

/// Agent -> TUI event.
pub enum AgentMsg {
    /// Streaming text chunk.
    Text(String),
    /// Response complete.
    Done,
}

pub async fn run_tui(
    board: Arc<Board>,
    operator: Arc<Operator>,
    mut log_rx: tokio::sync::mpsc::Receiver<LogEntry>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Initial board load
    if let Ok(items) = board.list().await {
        app.board.items = items;
    }
    load_rigs(&board, &mut app).await;

    // Agent communication channel
    let (agent_tx, mut agent_rx) = mpsc::channel::<AgentMsg>(100);

    // crossterm event stream
    let mut reader = EventStream::new();

    // Board refresh timer (2s)
    let mut board_tick = interval(Duration::from_secs(2));

    // Render timer (50ms)
    let mut render_tick = interval(Duration::from_millis(50));

    // Initial render
    terminal.draw(|f| ui::render(f, &app))?;

    loop {
        tokio::select! {
            // Keyboard input
            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if handle_key(key, &mut app, &agent_tx, &board, &operator).await {
                            break;
                        }
                    }
                    Some(Ok(Event::Resize(_, _))) => {
                        // Redraw on resize
                    }
                    Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            // Agent response
            Some(msg) = agent_rx.recv() => {
                match msg {
                    AgentMsg::Text(text) => {
                        app.append_agent_text(&text);
                    }
                    AgentMsg::Done => {
                        app.agent_busy = false;
                        if let Ok(items) = board.list().await {
                            app.board.items = items;
                        }
                    }
                }
            }
            // Log entries
            Some(entry) = log_rx.recv() => {
                app.push_log(entry);
            }
            // Periodic board refresh
            _ = board_tick.tick() => {
                if let Ok(items) = board.list().await {
                    app.board.items = items;
                }
                load_rigs(&board, &mut app).await;
            }
            // Render
            _ = render_tick.tick() => {
                terminal.draw(|f| ui::render(f, &app))?;
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tui::app::RigStatus;

    #[test]
    fn rig_status_icons_used_in_ui() {
        assert_eq!(RigStatus::Idle.icon(), "💤");
        assert_eq!(RigStatus::Working.icon(), "⚙");
    }
}
