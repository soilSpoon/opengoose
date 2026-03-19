use opengoose_board::work_item::{Status, WorkItem};

/// TUI 전체 상태
pub struct App {
    pub board_items: Vec<WorkItem>,
    pub rigs: Vec<RigInfo>,
    pub chat_lines: Vec<ChatLine>,
    pub input: String,
    pub cursor_pos: usize,
    pub scroll_offset: u16,
    pub should_quit: bool,
    pub agent_busy: bool,
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
        }
    }

    pub fn board_summary(&self) -> (usize, usize, usize) {
        let open = self.board_items.iter().filter(|i| i.status == Status::Open).count();
        let claimed = self.board_items.iter().filter(|i| i.status == Status::Claimed).count();
        let done = self.board_items.iter().filter(|i| i.status == Status::Done).count();
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
}

impl RigStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            RigStatus::Idle => "💤",
            RigStatus::Working => "⚙",
        }
    }
}
