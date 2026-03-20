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

/// TUI 전체 상태
pub struct App {
    pub board_items: Vec<WorkItem>,
    pub rigs: Vec<RigInfo>,
    pub chat_lines: Vec<ChatLine>,
    pub input: String,
    pub cursor_pos: usize,
    pub scroll_offset: usize,
    pub should_quit: bool,
    pub agent_busy: bool,
    pub current_tab: Tab,
    pub tab_bar_visible: bool,
    pub log_entries: VecDeque<LogEntry>,
    pub log_verbose: bool,
    pub log_scroll_offset: usize,
    pub log_auto_scroll: bool,
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

impl App {
    pub fn new() -> Self {
        Self {
            board_items: Vec::new(),
            rigs: Vec::new(),
            chat_lines: vec![ChatLine::System("OpenGoose v0.2 — TUI".into())],
            input: String::new(),
            cursor_pos: 0,
            scroll_offset: 0,
            should_quit: false,
            agent_busy: false,
            current_tab: Tab::Chat,
            tab_bar_visible: true,
            log_entries: VecDeque::new(),
            log_verbose: false,
            log_scroll_offset: 0,
            log_auto_scroll: true,
        }
    }

    pub fn board_summary(&self) -> (usize, usize, usize) {
        let open = self
            .board_items
            .iter()
            .filter(|i| i.status == Status::Open)
            .count();
        let claimed = self
            .board_items
            .iter()
            .filter(|i| i.status == Status::Claimed)
            .count();
        let done = self
            .board_items
            .iter()
            .filter(|i| i.status == Status::Done)
            .count();
        (open, claimed, done)
    }

    pub fn push_chat(&mut self, line: ChatLine) {
        self.chat_lines.push(line);
        // 자동 스크롤: 새 메시지가 오면 맨 아래로
        self.scroll_offset = 0;
    }

    pub fn append_agent_text(&mut self, text: &str) {
        // 마지막 줄이 Agent이면 이어붙이기 (스트리밍)
        if let Some(ChatLine::Agent(last)) = self.chat_lines.last_mut() {
            last.push_str(text);
        } else {
            self.chat_lines.push(ChatLine::Agent(text.to_string()));
        }
        self.scroll_offset = 0;
    }

    /// cursor_pos(문자 인덱스) → 바이트 인덱스
    pub fn cursor_byte_pos(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }

    /// 문자 개수
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
        self.push_chat(ChatLine::User(text.clone()));
        Some(text)
    }

    /// 활성 작업 목록 (Open + Claimed, priority 순)
    pub fn active_items(&self) -> Vec<&WorkItem> {
        let mut items: Vec<_> = self
            .board_items
            .iter()
            .filter(|i| i.status == Status::Open || i.status == Status::Claimed)
            .collect();
        items.sort_by_key(|i| i.priority.urgency());
        items.reverse();
        items
    }

    /// 최근 완료 항목 (최대 3개)
    pub fn recent_done(&self) -> Vec<&WorkItem> {
        self.board_items
            .iter()
            .filter(|i| i.status == Status::Done)
            .rev()
            .take(3)
            .collect()
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        if self.log_entries.len() >= 1000 {
            self.log_entries.pop_front();
        }
        self.log_entries.push_back(entry);
        if self.log_auto_scroll {
            self.log_scroll_offset = 0;
        }
    }

    pub fn visible_logs(&self) -> Vec<&LogEntry> {
        if self.log_verbose {
            self.log_entries.iter().collect()
        } else {
            self.log_entries.iter().filter(|e| e.structured).collect()
        }
    }
}

impl RigStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            RigStatus::Idle => "💤",
            RigStatus::Working => "⚙",
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
        assert_eq!(app.board_items.len(), 0);
        assert_eq!(app.rigs.len(), 0);
        assert_eq!(app.chat_lines.len(), 1);
        assert_eq!(app.input, "");
        assert_eq!(app.cursor_pos, 0);
        assert_eq!(app.scroll_offset, 0);
        assert!(!app.should_quit);
        assert!(!app.agent_busy);

        assert!(
            matches!(app.chat_lines.first(), Some(ChatLine::System(s)) if s.contains("OpenGoose"))
        );
    }

    #[test]
    fn board_summary_counts_open_claimed_done() {
        let app = App {
            board_items: vec![
                make_item(1, "open", Status::Open, Priority::P1, None),
                make_item(2, "claimed", Status::Claimed, Priority::P1, Some("r1")),
                make_item(3, "done", Status::Done, Priority::P1, Some("r1")),
            ],
            ..App::new()
        };
        let (open, claimed, done) = app.board_summary();
        assert_eq!(open, 1);
        assert_eq!(claimed, 1);
        assert_eq!(done, 1);
    }

    #[test]
    fn chat_behaviors() {
        let mut app = App::new();
        app.scroll_offset = 4;
        app.push_chat(ChatLine::System("ok".into()));
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.chat_lines.len(), 2);

        app.append_agent_text("hello");
        app.append_agent_text(" world");
        match app.chat_lines.last() {
            Some(ChatLine::Agent(text)) => assert_eq!(text, "hello world"),
            _ => panic!("expected agent text"),
        }
        app.scroll_offset = 3;
        app.push_chat(ChatLine::System("reset".into()));
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn cursor_byte_pos_and_char_count() {
        let mut app = App::new();
        app.input = "a한글".into();
        assert_eq!(app.char_count(), 3);
        app.cursor_pos = 0;
        assert_eq!(app.cursor_byte_pos(), 0);
        app.cursor_pos = 1;
        assert_eq!(app.cursor_byte_pos(), 1); // after 'a'
        app.cursor_pos = 2;
        assert_eq!(app.cursor_byte_pos(), 4); // after 'a' + first CJK char
        app.cursor_pos = 3;
        assert_eq!(app.cursor_byte_pos(), app.input.len());
    }

    #[test]
    fn submit_input_trims_and_pushes_user_line() {
        let mut app = App::new();
        app.input = "  hello ".into();
        let out = app.submit_input();
        assert_eq!(out, Some("hello".into()));
        assert_eq!(app.input, "");
        assert_eq!(app.cursor_pos, 0);
        assert!(matches!(app.chat_lines.last(), Some(ChatLine::User(text)) if text == "hello"));
    }

    #[test]
    fn active_items_are_sorted_by_priority() {
        let app = App {
            board_items: vec![
                make_item(1, "low", Status::Open, Priority::P2, None),
                make_item(2, "high", Status::Claimed, Priority::P0, None),
                make_item(3, "mid", Status::Open, Priority::P1, None),
            ],
            ..App::new()
        };
        let active = app.active_items();
        assert_eq!(
            active.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }

    #[test]
    fn recent_done_limits_to_three_latest() {
        let app = App {
            board_items: vec![
                make_item(1, "done-1", Status::Done, Priority::P1, None),
                make_item(2, "done-2", Status::Done, Priority::P1, None),
                make_item(3, "done-3", Status::Done, Priority::P1, None),
                make_item(4, "done-4", Status::Done, Priority::P1, None),
            ],
            ..App::new()
        };
        let recent = app.recent_done();
        assert_eq!(
            recent.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![4, 3, 2]
        );
    }

    #[test]
    fn rig_status_icons() {
        assert_eq!(RigStatus::Idle.icon(), "💤");
        assert_eq!(RigStatus::Working.icon(), "⚙");
    }
}
