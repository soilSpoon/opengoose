use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::Tab;

/// All possible TUI key commands, decoupled from side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCommand {
    /// Ctrl+C — quit the TUI.
    Quit,
    /// Jump directly to a tab (Ctrl+1/2/3).
    GoToTab(Tab),
    /// Tab key — next tab.
    TabNext,
    /// Shift+Tab — previous tab.
    TabPrev,
    /// Ctrl+\ — toggle tab bar visibility.
    ToggleTabBar,

    // ── Scroll ──
    /// Scroll up by 1 line (Up arrow, context-dependent).
    ScrollUp,
    /// Scroll down by 1 line (Down arrow, context-dependent).
    ScrollDown,
    /// Scroll up by a page (PageUp).
    PageUp,
    /// Scroll down by a page (PageDown).
    PageDown,

    // ── Chat editing ──
    /// Submit the current input (Enter).
    Submit,
    /// Insert a character at cursor.
    InsertChar(char),
    /// Delete character before cursor (Backspace).
    Backspace,
    /// Delete character at cursor (Delete).
    Delete,
    /// Move cursor left.
    CursorLeft,
    /// Move cursor right.
    CursorRight,
    /// Move cursor to start of input.
    CursorHome,
    /// Move cursor to end of input.
    CursorEnd,

    // ── Logs ──
    /// Toggle verbose log display.
    ToggleLogVerbose,
}

/// Context passed to dispatch so it can make input-mode-sensitive decisions.
pub struct KeyContext {
    pub current_tab: Tab,
    pub chat_input_empty: bool,
}

/// Pure function: maps a key event + context to an optional command.
/// Returns `None` when the key has no binding in the current context.
pub fn dispatch(key: KeyEvent, ctx: &KeyContext) -> Option<KeyCommand> {
    match (key.code, key.modifiers) {
        // ── Global shortcuts ──
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(KeyCommand::Quit),
        (KeyCode::Char('1'), KeyModifiers::CONTROL) => Some(KeyCommand::GoToTab(Tab::Chat)),
        (KeyCode::Char('2'), KeyModifiers::CONTROL) => Some(KeyCommand::GoToTab(Tab::Board)),
        (KeyCode::Char('3'), KeyModifiers::CONTROL) => Some(KeyCommand::GoToTab(Tab::Logs)),
        (KeyCode::Tab, KeyModifiers::NONE) => Some(KeyCommand::TabNext),
        (KeyCode::BackTab, _) => Some(KeyCommand::TabPrev),
        (KeyCode::Char('\\'), KeyModifiers::CONTROL) => Some(KeyCommand::ToggleTabBar),

        // ── Scroll (per tab) ──
        (KeyCode::Up, KeyModifiers::NONE) => match ctx.current_tab {
            Tab::Chat if ctx.chat_input_empty => Some(KeyCommand::ScrollUp),
            Tab::Logs => Some(KeyCommand::ScrollUp),
            _ => None,
        },
        (KeyCode::Down, KeyModifiers::NONE) => match ctx.current_tab {
            Tab::Chat if ctx.chat_input_empty => Some(KeyCommand::ScrollDown),
            Tab::Logs => Some(KeyCommand::ScrollDown),
            _ => None,
        },
        (KeyCode::PageUp, _) => match ctx.current_tab {
            Tab::Chat | Tab::Logs => Some(KeyCommand::PageUp),
            _ => None,
        },
        (KeyCode::PageDown, _) => match ctx.current_tab {
            Tab::Chat | Tab::Logs => Some(KeyCommand::PageDown),
            _ => None,
        },

        // ── Tab-specific ──
        _ => match ctx.current_tab {
            Tab::Chat => dispatch_chat(key),
            Tab::Logs => dispatch_logs(key),
            Tab::Board => None,
        },
    }
}

/// Chat-tab key bindings (editing).
fn dispatch_chat(key: KeyEvent) -> Option<KeyCommand> {
    match key.code {
        KeyCode::Enter => Some(KeyCommand::Submit),
        KeyCode::Char(c)
            if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
        {
            Some(KeyCommand::InsertChar(c))
        }
        KeyCode::Backspace => Some(KeyCommand::Backspace),
        KeyCode::Delete => Some(KeyCommand::Delete),
        KeyCode::Left => Some(KeyCommand::CursorLeft),
        KeyCode::Right => Some(KeyCommand::CursorRight),
        KeyCode::Home => Some(KeyCommand::CursorHome),
        KeyCode::End => Some(KeyCommand::CursorEnd),
        _ => None,
    }
}

/// Logs-tab key bindings.
fn dispatch_logs(key: KeyEvent) -> Option<KeyCommand> {
    match key.code {
        KeyCode::Char('v') => Some(KeyCommand::ToggleLogVerbose),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn ctx(tab: Tab, chat_input_empty: bool) -> KeyContext {
        KeyContext {
            current_tab: tab,
            chat_input_empty,
        }
    }

    // ── Global shortcuts ──

    #[test]
    fn ctrl_c_quits() {
        let cmd = dispatch(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &ctx(Tab::Chat, true),
        );
        assert_eq!(cmd, Some(KeyCommand::Quit));
    }

    #[test]
    fn ctrl_1_2_3_jump_to_tabs() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Char('1'), KeyModifiers::CONTROL),
                &ctx(Tab::Logs, true),
            ),
            Some(KeyCommand::GoToTab(Tab::Chat))
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Char('2'), KeyModifiers::CONTROL),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::GoToTab(Tab::Board))
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Char('3'), KeyModifiers::CONTROL),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::GoToTab(Tab::Logs))
        );
    }

    #[test]
    fn tab_and_backtab_cycle() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::TabNext)
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::TabPrev)
        );
    }

    #[test]
    fn ctrl_backslash_toggles_tab_bar() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::ToggleTabBar)
        );
    }

    // ── Scroll ──

    #[test]
    fn up_down_scroll_in_chat_when_input_empty() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::ScrollUp)
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::ScrollDown)
        );
    }

    #[test]
    fn up_down_noop_in_chat_when_input_nonempty() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                &ctx(Tab::Chat, false),
            ),
            None
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                &ctx(Tab::Chat, false),
            ),
            None
        );
    }

    #[test]
    fn up_down_scroll_in_logs() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                &ctx(Tab::Logs, true),
            ),
            Some(KeyCommand::ScrollUp)
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                &ctx(Tab::Logs, true),
            ),
            Some(KeyCommand::ScrollDown)
        );
    }

    #[test]
    fn pageup_pagedown_in_chat_and_logs() {
        for tab in [Tab::Chat, Tab::Logs] {
            assert_eq!(
                dispatch(
                    KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
                    &ctx(tab, true),
                ),
                Some(KeyCommand::PageUp)
            );
            assert_eq!(
                dispatch(
                    KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
                    &ctx(tab, true),
                ),
                Some(KeyCommand::PageDown)
            );
        }
    }

    #[test]
    fn pageup_pagedown_noop_in_board() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
                &ctx(Tab::Board, true),
            ),
            None
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
                &ctx(Tab::Board, true),
            ),
            None
        );
    }

    // ── Chat editing ──

    #[test]
    fn chat_enter_submits() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::Submit)
        );
    }

    #[test]
    fn chat_char_inserts() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::InsertChar('a'))
        );
        // Shift chars also insert
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT),
                &ctx(Tab::Chat, true),
            ),
            Some(KeyCommand::InsertChar('A'))
        );
    }

    #[test]
    fn chat_backspace_and_delete() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                &ctx(Tab::Chat, false),
            ),
            Some(KeyCommand::Backspace)
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
                &ctx(Tab::Chat, false),
            ),
            Some(KeyCommand::Delete)
        );
    }

    #[test]
    fn chat_cursor_movement() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
                &ctx(Tab::Chat, false),
            ),
            Some(KeyCommand::CursorLeft)
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
                &ctx(Tab::Chat, false),
            ),
            Some(KeyCommand::CursorRight)
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
                &ctx(Tab::Chat, false),
            ),
            Some(KeyCommand::CursorHome)
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
                &ctx(Tab::Chat, false),
            ),
            Some(KeyCommand::CursorEnd)
        );
    }

    // ── Logs ──

    #[test]
    fn logs_v_toggles_verbose() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
                &ctx(Tab::Logs, true),
            ),
            Some(KeyCommand::ToggleLogVerbose)
        );
    }

    #[test]
    fn unknown_key_returns_none() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
                &ctx(Tab::Chat, true),
            ),
            None
        );
    }

    // ── Board tab ignores most keys ──

    #[test]
    fn board_tab_ignores_typing() {
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
                &ctx(Tab::Board, true),
            ),
            None
        );
        assert_eq!(
            dispatch(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &ctx(Tab::Board, true),
            ),
            None
        );
    }

    #[test]
    fn global_shortcuts_work_on_any_tab() {
        for tab in Tab::ALL {
            assert_eq!(
                dispatch(
                    KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                    &ctx(tab, true),
                ),
                Some(KeyCommand::Quit),
                "Ctrl+C should quit on {tab:?}"
            );
            assert_eq!(
                dispatch(
                    KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
                    &ctx(tab, true),
                ),
                Some(KeyCommand::TabNext),
                "Tab should cycle on {tab:?}"
            );
        }
    }
}
