pub enum ChatLine {
    User(String),
    Agent(String),
    System(String),
}

pub struct ChatState {
    pub lines: Vec<ChatLine>,
    pub input: String,
    pub cursor_pos: usize,
    pub scroll_offset: usize,
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

#[cfg(test)]
mod tests {
    use super::super::App;
    use super::*;

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
