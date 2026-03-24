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
                let ch = app.chat.input[byte_pos..]
                    .chars()
                    .next()
                    .expect("cursor_pos > 0 guarantees non-empty slice");
                app.chat
                    .input
                    .replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        KeyCode::Delete => {
            if app.chat.cursor_pos < app.char_count() {
                let byte_pos = app.cursor_byte_pos();
                let ch = app.chat.input[byte_pos..]
                    .chars()
                    .next()
                    .expect("cursor_pos < char_count guarantees non-empty slice");
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

#[cfg(test)]
mod tests {
    use super::handle_key;
    use crate::tui::app::{App, ChatLine, Tab};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use opengoose_board::work_item::RigId;
    use opengoose_rig::rig::Operator;
    use tokio::sync::mpsc;

    fn make_operator(session_id: &str) -> std::sync::Arc<Operator> {
        std::sync::Arc::new(Operator::without_board(
            RigId::new("test"),
            goose::agents::Agent::new(),
            session_id,
        ))
    }

    #[tokio::test]
    async fn handle_key_char_and_backspace() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s1");
        let (tx, _rx) = mpsc::channel(4);

        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!should_quit);
        assert_eq!(app.chat.input, "a");
        assert_eq!(app.chat.cursor_pos, 1);

        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!should_quit);
        assert_eq!(app.chat.input, "");
        assert_eq!(app.chat.cursor_pos, 0);
    }

    #[tokio::test]
    async fn handle_key_escape_and_scrolling() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s1");
        let (tx, _rx) = mpsc::channel(4);

        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(should_quit);
        assert!(app.should_quit);
        assert_eq!(app.chat.scroll_offset, 0);
    }

    #[tokio::test]
    async fn handle_key_scroll_keys_when_input_empty() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.scroll_offset = 0;
        handle_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 1);

        app.chat.scroll_offset = 3;
        handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 2);
    }

    #[tokio::test]
    async fn handle_key_tab_cycles_forward() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("tab1");
        let (tx, _rx) = mpsc::channel(4);

        assert_eq!(app.current_tab, Tab::Chat);
        handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Board);
        handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Logs);
        handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Chat);
    }

    #[tokio::test]
    async fn handle_key_backtab_cycles_backward() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("tab2");
        let (tx, _rx) = mpsc::channel(4);

        handle_key(
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Logs);
        handle_key(
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Board);
    }

    #[tokio::test]
    async fn handle_key_ctrl_1_2_3_jump_to_tabs() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("tab3");
        let (tx, _rx) = mpsc::channel(4);

        handle_key(
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Board);
        handle_key(
            KeyEvent::new(KeyCode::Char('3'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Logs);
        handle_key(
            KeyEvent::new(KeyCode::Char('1'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Chat);
    }

    #[tokio::test]
    async fn handle_key_ctrl_backslash_toggles_tab_bar() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("tabbar1");
        let (tx, _rx) = mpsc::channel(4);

        assert!(app.tab_bar_visible);
        handle_key(
            KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!app.tab_bar_visible);
        handle_key(
            KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app.tab_bar_visible);
    }

    #[tokio::test]
    async fn handle_key_v_toggles_log_verbose_in_logs_tab() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("verbose1");
        let (tx, _rx) = mpsc::channel(4);

        app.current_tab = Tab::Logs;
        app.logs.scroll_offset = 5;
        handle_key(
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app.logs.verbose);
        assert_eq!(app.logs.scroll_offset, 0);
        handle_key(
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!app.logs.verbose);
    }

    #[tokio::test]
    async fn handle_key_pageup_pagedown_in_chat_tab() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("page1");
        let (tx, _rx) = mpsc::channel(4);

        handle_key(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 10);
        handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 0);
        handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 0);
    }

    #[tokio::test]
    async fn handle_key_pageup_pagedown_in_logs_tab() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("page2");
        let (tx, _rx) = mpsc::channel(4);

        app.current_tab = Tab::Logs;
        app.logs.auto_scroll = true;
        handle_key(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.logs.scroll_offset, 10);
        assert!(!app.logs.auto_scroll);
        handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.logs.scroll_offset, 0);
        assert!(app.logs.auto_scroll);
    }

    #[tokio::test]
    async fn handle_key_pageup_pagedown_in_board_tab_are_noop() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("boardpage1");
        let (tx, _rx) = mpsc::channel(4);

        app.current_tab = Tab::Board;
        handle_key(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 0);
        assert_eq!(app.logs.scroll_offset, 0);
        handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 0);
        assert_eq!(app.logs.scroll_offset, 0);
    }

    #[tokio::test]
    async fn handle_key_up_down_in_logs_tab() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("logscroll1");
        let (tx, _rx) = mpsc::channel(4);

        app.current_tab = Tab::Logs;
        app.logs.auto_scroll = true;
        handle_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.logs.scroll_offset, 1);
        assert!(!app.logs.auto_scroll);
        handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.logs.scroll_offset, 0);
        assert!(app.logs.auto_scroll);
    }

    #[tokio::test]
    async fn handle_key_up_down_in_chat_tab_with_nonempty_input_are_noop() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("chatscroll1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "typing".into();
        app.chat.cursor_pos = 6;
        app.chat.scroll_offset = 2;
        handle_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 2);
        handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 2);
    }

    #[tokio::test]
    async fn handle_key_left_right_home_end_cursor_movement() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("cursor1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "hello".into();
        app.chat.cursor_pos = 5;

        handle_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 4);
        handle_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 5);
        handle_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 5); // at end, no-op
        handle_key(
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 0);
        handle_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 0); // saturating_sub
        handle_key(
            KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 5);
    }

    #[tokio::test]
    async fn handle_key_delete_removes_char_at_cursor() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("delete1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "abc".into();
        app.chat.cursor_pos = 1;
        handle_key(
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.input, "ac");
        app.chat.cursor_pos = 2;
        handle_key(
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.input, "ac"); // at end, no-op
    }

    #[tokio::test]
    async fn handle_key_enter_with_empty_input_is_noop() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("enter_empty");
        let (tx, _rx) = mpsc::channel(4);

        let initial_lines = app.chat.lines.len();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.lines.len(), initial_lines);
        assert!(!app.agent_busy);
    }

    #[tokio::test]
    async fn handle_key_enter_when_not_busy_sets_agent_busy() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("notbusy1");
        let (tx, _rx) = mpsc::channel(16);

        app.chat.input = "hello there".into();
        app.agent_busy = false;
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app.agent_busy);
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::User(text) if text == "hello there"))
        );
    }

    #[tokio::test]
    async fn handle_key_unknown_key_hits_default_branch() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("unknown1");
        let (tx, _rx) = mpsc::channel(4);

        let should_quit = handle_key(
            KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!should_quit);
    }

    #[tokio::test]
    async fn handle_key_busy_chat_does_not_send_to_agent() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s4");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "hello".into();
        app.agent_busy = true;
        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(!should_quit);
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::System(text) if text == "Agent is busy..."))
        );
    }
}
