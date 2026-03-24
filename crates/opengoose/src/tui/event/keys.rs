use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use opengoose_board::Board;
use opengoose_rig::rig::Operator;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::AgentMsg;
use super::commands::handle_input;
use crate::tui::app::{App, Tab};

/// Key event dispatch. Returns true if TUI should quit.
pub async fn handle_key(
    key: KeyEvent,
    app: &mut App,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) -> bool {
    match (key.code, key.modifiers) {
        // ── Global shortcuts ──
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return true;
        }
        (KeyCode::Char('1'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Chat;
        }
        (KeyCode::Char('2'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Board;
        }
        (KeyCode::Char('3'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Logs;
        }
        (KeyCode::Tab, KeyModifiers::NONE) => {
            app.current_tab = app.current_tab.next();
        }
        (KeyCode::BackTab, _) => {
            app.current_tab = app.current_tab.prev();
        }
        (KeyCode::Char('\\'), KeyModifiers::CONTROL) => {
            app.tab_bar_visible = !app.tab_bar_visible;
        }

        // ── Scroll (per tab) ──
        (KeyCode::Up, KeyModifiers::NONE) => match app.current_tab {
            Tab::Chat if app.chat.input.is_empty() => {
                app.chat.scroll_offset = app.chat.scroll_offset.saturating_add(1);
            }
            Tab::Logs => {
                app.logs.scroll_offset = app.logs.scroll_offset.saturating_add(1);
                app.logs.auto_scroll = false;
            }
            _ => {}
        },
        (KeyCode::Down, KeyModifiers::NONE) => match app.current_tab {
            Tab::Chat if app.chat.input.is_empty() => {
                app.chat.scroll_offset = app.chat.scroll_offset.saturating_sub(1);
            }
            Tab::Logs => {
                app.logs.scroll_offset = app.logs.scroll_offset.saturating_sub(1);
                if app.logs.scroll_offset == 0 {
                    app.logs.auto_scroll = true;
                }
            }
            _ => {}
        },
        (KeyCode::PageUp, _) => match app.current_tab {
            Tab::Chat => {
                app.chat.scroll_offset = app.chat.scroll_offset.saturating_add(10);
            }
            Tab::Logs => {
                app.logs.scroll_offset = app.logs.scroll_offset.saturating_add(10);
                app.logs.auto_scroll = false;
            }
            _ => {}
        },
        (KeyCode::PageDown, _) => match app.current_tab {
            Tab::Chat => {
                app.chat.scroll_offset = app.chat.scroll_offset.saturating_sub(10);
            }
            Tab::Logs => {
                app.logs.scroll_offset = app.logs.scroll_offset.saturating_sub(10);
                if app.logs.scroll_offset == 0 {
                    app.logs.auto_scroll = true;
                }
            }
            _ => {}
        },

        // ── Tab-specific key handling ──
        _ => match app.current_tab {
            Tab::Chat => handle_chat_key(key, app, agent_tx, board, operator).await,
            Tab::Board => {}
            Tab::Logs => handle_logs_key(key, app),
        },
    }

    false
}

/// Chat tab key handling.
async fn handle_chat_key(
    key: KeyEvent,
    app: &mut App,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) {
    match key.code {
        KeyCode::Enter => {
            if let Some(text) = app.submit_input() {
                handle_input(app, &text, agent_tx, board, operator).await;
            }
        }
        KeyCode::Char(c)
            if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
        {
            let byte_pos = app.cursor_byte_pos();
            app.chat.input.insert(byte_pos, c);
            app.chat.cursor_pos += 1;
        }
        KeyCode::Backspace => {
            if app.chat.cursor_pos > 0 {
                app.chat.cursor_pos -= 1;
                let byte_pos = app.cursor_byte_pos();
                let ch = app.chat.input[byte_pos..].chars().next().expect("cursor_pos > 0 guarantees non-empty slice");
                app.chat
                    .input
                    .replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        KeyCode::Delete => {
            if app.chat.cursor_pos < app.char_count() {
                let byte_pos = app.cursor_byte_pos();
                let ch = app.chat.input[byte_pos..].chars().next().expect("cursor_pos < char_count guarantees non-empty slice");
                app.chat
                    .input
                    .replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        KeyCode::Left => app.chat.cursor_pos = app.chat.cursor_pos.saturating_sub(1),
        KeyCode::Right => {
            if app.chat.cursor_pos < app.char_count() {
                app.chat.cursor_pos += 1;
            }
        }
        KeyCode::Home => app.chat.cursor_pos = 0,
        KeyCode::End => app.chat.cursor_pos = app.char_count(),
        _ => {}
    }
}

/// Logs tab key handling.
fn handle_logs_key(key: KeyEvent, app: &mut App) {
    if let KeyCode::Char('v') = key.code {
        app.logs.verbose = !app.logs.verbose;
        app.logs.scroll_offset = 0;
    }
}
