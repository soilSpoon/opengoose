mod board;
mod chat;
mod logs;

pub use board::BoardState;
pub use chat::{ChatLine, ChatState};
pub use logs::LogState;

use crate::tui::log_entry::LogEntry;
use opengoose_board::work_item::WorkItem;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Chat,
    Board,
    Logs,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Tab::Chat => Tab::Board,
            Tab::Board => Tab::Logs,
            Tab::Logs => Tab::Chat,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::Chat => Tab::Logs,
            Tab::Board => Tab::Chat,
            Tab::Logs => Tab::Board,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Chat => "Chat",
            Tab::Board => "Board",
            Tab::Logs => "Logs",
        }
    }

    pub const ALL: [Tab; 3] = [Tab::Chat, Tab::Board, Tab::Logs];
}

/// TUI 전체 상태
pub struct App {
    pub chat: ChatState,
    pub board: BoardState,
    pub logs: LogState,
    pub should_quit: bool,
    pub agent_busy: bool,
    pub current_tab: Tab,
    pub tab_bar_visible: bool,
}

/// Rig 요약 (Board DB에서 가져온 정보)
pub struct RigInfo {
    pub id: String,
    pub trust_level: String,
    pub status: RigStatus,
}

pub enum RigStatus {
    Idle,
    Working,
}

impl App {
    pub fn new() -> Self {
        Self {
            chat: ChatState {
                lines: vec![ChatLine::System("OpenGoose v0.2 — TUI".into())],
                input: String::new(),
                cursor_pos: 0,
                scroll_offset: 0,
            },
            board: BoardState {
                items: Vec::new(),
                rigs: Vec::new(),
            },
            logs: LogState {
                entries: VecDeque::new(),
                verbose: false,
                scroll_offset: 0,
                auto_scroll: true,
            },
            should_quit: false,
            agent_busy: false,
            current_tab: Tab::Chat,
            tab_bar_visible: true,
        }
    }

    pub fn board_summary(&self) -> (usize, usize, usize) {
        self.board.summary()
    }

    pub fn push_chat(&mut self, line: ChatLine) {
        self.chat.push(line);
    }

    pub fn append_agent_text(&mut self, text: &str) {
        self.chat.append_agent_text(text);
    }

    pub fn cursor_byte_pos(&self) -> usize {
        self.chat.cursor_byte_pos()
    }

    pub fn char_count(&self) -> usize {
        self.chat.char_count()
    }

    pub fn submit_input(&mut self) -> Option<String> {
        self.chat.submit_input()
    }

    pub fn active_items(&self) -> Vec<&WorkItem> {
        self.board.active_items()
    }

    pub fn recent_done(&self) -> Vec<&WorkItem> {
        self.board.recent_done()
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        self.logs.push(entry);
    }

    pub fn visible_logs(&self) -> Vec<&LogEntry> {
        self.logs.visible()
    }
}

impl RigStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            RigStatus::Idle => "\u{1f4a4}",
            RigStatus::Working => "\u{2699}",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_initial_state() {
        let app = App::new();
        assert_eq!(app.board.items.len(), 0);
        assert_eq!(app.board.rigs.len(), 0);
        assert_eq!(app.chat.lines.len(), 1);
        assert_eq!(app.chat.input, "");
        assert_eq!(app.chat.cursor_pos, 0);
        assert_eq!(app.chat.scroll_offset, 0);
        assert!(!app.should_quit);
        assert!(!app.agent_busy);

        assert!(
            matches!(app.chat.lines.first(), Some(ChatLine::System(s)) if s.contains("OpenGoose"))
        );
    }

    #[test]
    fn rig_status_icons() {
        assert_eq!(RigStatus::Idle.icon(), "\u{1f4a4}");
        assert_eq!(RigStatus::Working.icon(), "\u{2699}");
    }

    #[test]
    fn tab_next_cycles_forward() {
        assert_eq!(Tab::Chat.next(), Tab::Board);
        assert_eq!(Tab::Board.next(), Tab::Logs);
        assert_eq!(Tab::Logs.next(), Tab::Chat);
    }

    #[test]
    fn tab_prev_cycles_backward() {
        assert_eq!(Tab::Chat.prev(), Tab::Logs);
        assert_eq!(Tab::Board.prev(), Tab::Chat);
        assert_eq!(Tab::Logs.prev(), Tab::Board);
    }

    #[test]
    fn tab_label_returns_correct_strings() {
        assert_eq!(Tab::Chat.label(), "Chat");
        assert_eq!(Tab::Board.label(), "Board");
        assert_eq!(Tab::Logs.label(), "Logs");
    }
}
