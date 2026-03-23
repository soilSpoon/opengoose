use super::log_entry::LogEntry;
use opengoose_board::work_item::{Status, WorkItem};
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

pub struct ChatState {
    pub lines: Vec<ChatLine>,
    pub input: String,
    pub cursor_pos: usize,
    pub scroll_offset: usize,
}

pub struct BoardState {
    pub items: Vec<WorkItem>,
    pub rigs: Vec<RigInfo>,
}

pub struct LogState {
    pub entries: VecDeque<LogEntry>,
    pub verbose: bool,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
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

pub enum ChatLine {
    User(String),
    Agent(String),
    System(String),
}

impl ChatState {
    pub fn push(&mut self, line: ChatLine) {
        self.lines.push(line);
        self.scroll_offset = 0;
    }

    pub fn append_agent_text(&mut self, text: &str) {
        if let Some(ChatLine::Agent(last)) = self.lines.last_mut() {
            last.push_str(text);
        } else {
            self.lines.push(ChatLine::Agent(text.to_string()));
        }
        self.scroll_offset = 0;
    }

    pub fn cursor_byte_pos(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }

    pub fn char_count(&self) -> usize {
        self.input.chars().count()
    }

    pub fn submit_input(&mut self) -> Option<String> {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.input.clear();
        self.cursor_pos = 0;
        self.push(ChatLine::User(text.clone()));
        Some(text)
    }
}

impl BoardState {
    pub fn summary(&self) -> (usize, usize, usize) {
        let open = self
            .items
            .iter()
            .filter(|i| i.status == Status::Open)
            .count();
        let claimed = self
            .items
            .iter()
            .filter(|i| i.status == Status::Claimed)
            .count();
        let done = self
            .items
            .iter()
            .filter(|i| i.status == Status::Done)
            .count();
        (open, claimed, done)
    }

    pub fn active_items(&self) -> Vec<&WorkItem> {
        let mut items: Vec<_> = self
            .items
            .iter()
            .filter(|i| i.status == Status::Open || i.status == Status::Claimed)
            .collect();
        items.sort_by_key(|i| i.priority.urgency());
        items.reverse();
        items
    }

    pub fn recent_done(&self) -> Vec<&WorkItem> {
        self.items
            .iter()
            .filter(|i| i.status == Status::Done)
            .rev()
            .take(3)
            .collect()
    }
}

impl LogState {
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= 1000 {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        if self.auto_scroll {
            self.scroll_offset = 0;
        }
    }

    pub fn visible(&self) -> Vec<&LogEntry> {
        if self.verbose {
            self.entries.iter().collect()
        } else {
            self.entries.iter().filter(|e| e.structured).collect()
        }
    }
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
    use chrono::Utc;
    use opengoose_board::work_item::{Priority, RigId, Status, WorkItem};

    fn make_item(
        id: i64,
        title: &str,
        status: Status,
        priority: Priority,
        claimed_by: Option<&str>,
    ) -> WorkItem {
        WorkItem {
            id,
            title: title.into(),
            description: String::new(),
            created_by: RigId::new("test"),
            created_at: Utc::now(),
            status,
            priority,
            tags: Vec::new(),
            claimed_by: claimed_by.map(RigId::new),
            updated_at: Utc::now(),
        }
    }

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
    fn board_summary_counts_open_claimed_done() {
        let mut app = App::new();
        app.board.items = vec![
            make_item(1, "open", Status::Open, Priority::P1, None),
            make_item(2, "claimed", Status::Claimed, Priority::P1, Some("r1")),
            make_item(3, "done", Status::Done, Priority::P1, Some("r1")),
        ];
        let (open, claimed, done) = app.board_summary();
        assert_eq!(open, 1);
        assert_eq!(claimed, 1);
        assert_eq!(done, 1);
    }

    #[test]
    fn chat_behaviors() {
        let mut app = App::new();
        app.chat.scroll_offset = 4;
        app.push_chat(ChatLine::System("ok".into()));
        assert_eq!(app.chat.scroll_offset, 0);
        assert_eq!(app.chat.lines.len(), 2);

        app.append_agent_text("hello");
        app.append_agent_text(" world");
        match app.chat.lines.last() {
            Some(ChatLine::Agent(text)) => assert_eq!(text, "hello world"),
            _ => panic!("expected agent text"),
        }
        app.chat.scroll_offset = 3;
        app.push_chat(ChatLine::System("reset".into()));
        assert_eq!(app.chat.scroll_offset, 0);
    }

    #[test]
    fn cursor_byte_pos_and_char_count() {
        let mut app = App::new();
        app.chat.input = "a\u{d55c}\u{ae00}".into();
        assert_eq!(app.char_count(), 3);
        app.chat.cursor_pos = 0;
        assert_eq!(app.cursor_byte_pos(), 0);
        app.chat.cursor_pos = 1;
        assert_eq!(app.cursor_byte_pos(), 1); // after 'a'
        app.chat.cursor_pos = 2;
        assert_eq!(app.cursor_byte_pos(), 4); // after 'a' + first CJK char
        app.chat.cursor_pos = 3;
        assert_eq!(app.cursor_byte_pos(), app.chat.input.len());
    }

    #[test]
    fn submit_input_trims_and_pushes_user_line() {
        let mut app = App::new();
        app.chat.input = "  hello ".into();
        let out = app.submit_input();
        assert_eq!(out, Some("hello".into()));
        assert_eq!(app.chat.input, "");
        assert_eq!(app.chat.cursor_pos, 0);
        assert!(matches!(app.chat.lines.last(), Some(ChatLine::User(text)) if text == "hello"));
    }

    #[test]
    fn active_items_are_sorted_by_priority() {
        let mut app = App::new();
        app.board.items = vec![
            make_item(1, "low", Status::Open, Priority::P2, None),
            make_item(2, "high", Status::Claimed, Priority::P0, None),
            make_item(3, "mid", Status::Open, Priority::P1, None),
        ];
        let active = app.active_items();
        assert_eq!(
            active.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }

    #[test]
    fn recent_done_limits_to_three_latest() {
        let mut app = App::new();
        app.board.items = vec![
            make_item(1, "done-1", Status::Done, Priority::P1, None),
            make_item(2, "done-2", Status::Done, Priority::P1, None),
            make_item(3, "done-3", Status::Done, Priority::P1, None),
            make_item(4, "done-4", Status::Done, Priority::P1, None),
        ];
        let recent = app.recent_done();
        assert_eq!(
            recent.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![4, 3, 2]
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

    #[test]
    fn push_log_pops_front_when_at_limit() {
        use super::super::log_entry::LogEntry;
        let mut app = App::new();
        for i in 0..1000u64 {
            app.logs.entries.push_back(LogEntry {
                timestamp: Utc::now(),
                level: tracing::Level::INFO,
                target: format!("target-{i}"),
                message: format!("msg-{i}"),
                structured: true,
            });
        }
        assert_eq!(app.logs.entries.len(), 1000);
        app.push_log(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::INFO,
            target: "new".into(),
            message: "new msg".into(),
            structured: true,
        });
        assert_eq!(app.logs.entries.len(), 1000);
        assert_eq!(app.logs.entries.back().unwrap().target, "new");
    }

    #[test]
    fn push_log_does_not_reset_scroll_when_auto_scroll_false() {
        use super::super::log_entry::LogEntry;
        let mut app = App::new();
        app.logs.auto_scroll = false;
        app.logs.scroll_offset = 42;
        app.push_log(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::INFO,
            target: "t".into(),
            message: "m".into(),
            structured: true,
        });
        assert_eq!(app.logs.scroll_offset, 42);
    }

    #[test]
    fn visible_logs_filters_by_verbose_flag() {
        use super::super::log_entry::LogEntry;
        let mut app = App::new();
        app.logs.entries.push_back(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::INFO,
            target: "t".into(),
            message: "structured".into(),
            structured: true,
        });
        app.logs.entries.push_back(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::DEBUG,
            target: "t".into(),
            message: "unstructured".into(),
            structured: false,
        });

        app.logs.verbose = false;
        let visible = app.visible_logs();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].message, "structured");

        app.logs.verbose = true;
        let visible = app.visible_logs();
        assert_eq!(visible.len(), 2);
    }

    #[test]
    fn append_agent_text_creates_new_entry_when_last_is_not_agent() {
        let mut app = App::new();
        app.append_agent_text("hello");
        assert!(matches!(app.chat.lines.last(), Some(ChatLine::Agent(t)) if t == "hello"));
    }

    #[test]
    fn submit_input_returns_none_for_empty_input() {
        let mut app = App::new();
        app.chat.input = "   ".into();
        let result = app.submit_input();
        assert_eq!(result, None);
        assert!(matches!(app.chat.lines.last(), Some(ChatLine::System(_))));
    }
}
