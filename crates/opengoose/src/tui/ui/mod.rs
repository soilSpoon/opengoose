mod board;
mod chat;
mod logs;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::app::{App, Tab};

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if app.tab_bar_visible {
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        render_tab_bar(frame, app, chunks[0]);
        render_current_tab(frame, app, chunks[1]);
    } else {
        render_current_tab(frame, app, area);
    }
}

fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let tabs: Vec<Span> = Tab::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, tab)| {
            let style = if *tab == app.current_tab {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let mut spans = vec![Span::styled(format!(" {} ", tab.label()), style)];
            if i < Tab::ALL.len() - 1 {
                spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
            }
            spans
        })
        .collect();

    frame.render_widget(Paragraph::new(Line::from(tabs)), area);
}

fn render_current_tab(frame: &mut Frame, app: &App, area: Rect) {
    match app.current_tab {
        Tab::Chat => {
            let chunks = Layout::vertical([Constraint::Min(6), Constraint::Length(3)]).split(area);
            chat::render_chat(frame, app, chunks[0]);
            chat::render_input(frame, app, chunks[1]);
        }
        Tab::Board => {
            let chunks =
                Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(area);
            board::render_board(frame, app, chunks[0]);
            board::render_rigs(frame, app, chunks[1]);
        }
        Tab::Logs => {
            logs::render_logs(frame, app, area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::app::{App, ChatLine, RigInfo, RigStatus, Tab};
    use super::super::log_entry::LogEntry;
    use super::*;
    use chrono::Utc;
    use ratatui::{Terminal, backend::TestBackend};

    fn make_log_entry(level: tracing::Level, target: &str, structured: bool) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            level,
            target: target.to_string(),
            message: "test message".to_string(),
            structured,
        }
    }

    #[test]
    fn chat_line_to_lines_preserves_line_counts() {
        let user_line = ChatLine::User("hello".into());
        let user = chat::chat_line_to_lines(&user_line);
        assert_eq!(user.len(), 1);

        let agent_line = ChatLine::Agent("a\nb\nc".into());
        let agent = chat::chat_line_to_lines(&agent_line);
        assert_eq!(agent.len(), 3);

        let system_line = ChatLine::System("note".into());
        let system = chat::chat_line_to_lines(&system_line);
        assert_eq!(system.len(), 1);
    }

    #[test]
    fn format_log_entry_verbose_all_levels() {
        for level in [
            tracing::Level::ERROR,
            tracing::Level::WARN,
            tracing::Level::INFO,
            tracing::Level::DEBUG,
            tracing::Level::TRACE,
        ] {
            let entry = make_log_entry(level, "some::target", true);
            let line = logs::format_log_entry(&entry, true);
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.contains("test message"), "level={level}");
        }
    }

    #[test]
    fn format_log_entry_non_verbose_target_mapping() {
        let entry = make_log_entry(tracing::Level::INFO, "opengoose_rig::rig::foo", true);
        let line = logs::format_log_entry(&entry, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains("worker"),
            "expected 'worker' for ::rig target"
        );

        let entry = make_log_entry(tracing::Level::INFO, "opengoose::evolver", true);
        let line = logs::format_log_entry(&entry, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains("evolver"),
            "expected 'evolver' for evolver target"
        );

        let entry = make_log_entry(tracing::Level::INFO, "opengoose::web", true);
        let line = logs::format_log_entry(&entry, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains("system"),
            "expected 'system' for other target"
        );
    }

    #[test]
    fn render_with_tab_bar_visible_true() {
        let app = App::new();
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("terminal draw should succeed");
    }

    #[test]
    fn render_with_tab_bar_visible_false() {
        let mut app = App::new();
        app.tab_bar_visible = false;
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("terminal draw should succeed");
    }

    #[test]
    fn render_current_tab_board_with_items_and_rigs() {
        use opengoose_board::work_item::{Priority, RigId, Status, WorkItem};
        let mut app = App::new();
        app.current_tab = Tab::Board;
        app.board.items = vec![
            WorkItem {
                id: 1,
                title: "open item".into(),
                description: String::new(),
                created_by: RigId::new("test"),
                created_at: Utc::now(),
                status: Status::Open,
                priority: Priority::P1,
                tags: Vec::new(),
                claimed_by: None,
                updated_at: Utc::now(),
            },
            WorkItem {
                id: 2,
                title: "claimed item".into(),
                description: String::new(),
                created_by: RigId::new("test"),
                created_at: Utc::now(),
                status: Status::Claimed,
                priority: Priority::P0,
                tags: Vec::new(),
                claimed_by: Some(RigId::new("rig-1")),
                updated_at: Utc::now(),
            },
            WorkItem {
                id: 3,
                title: "done item".into(),
                description: String::new(),
                created_by: RigId::new("test"),
                created_at: Utc::now(),
                status: Status::Done,
                priority: Priority::P2,
                tags: Vec::new(),
                claimed_by: None,
                updated_at: Utc::now(),
            },
        ];
        app.board.rigs = vec![
            RigInfo {
                id: "rig-1".into(),
                trust_level: "L3".into(),
                status: RigStatus::Working,
            },
            RigInfo {
                id: "rig-2".into(),
                trust_level: "L2".into(),
                status: RigStatus::Idle,
            },
            RigInfo {
                id: "rig-3".into(),
                trust_level: "L1".into(),
                status: RigStatus::Idle,
            },
        ];
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("terminal draw should succeed");
    }

    #[test]
    fn render_current_tab_board_empty_rigs() {
        let mut app = App::new();
        app.current_tab = Tab::Board;
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("terminal draw should succeed");
    }

    #[test]
    fn render_current_tab_logs_verbose_and_non_verbose() {
        for verbose in [false, true] {
            let mut app = App::new();
            app.current_tab = Tab::Logs;
            app.logs.verbose = verbose;
            app.logs.entries.push_back(make_log_entry(
                tracing::Level::INFO,
                "opengoose::evolver",
                true,
            ));
            app.logs.entries.push_back(make_log_entry(
                tracing::Level::DEBUG,
                "opengoose_rig::rig",
                false,
            ));
            app.logs.entries.push_back(make_log_entry(
                tracing::Level::ERROR,
                "opengoose::web",
                true,
            ));
            let backend = TestBackend::new(80, 25);
            let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
            terminal
                .draw(|frame| render(frame, &app))
                .expect("terminal draw should succeed");
        }
    }

    #[test]
    fn render_logs_with_nonzero_scroll_offset() {
        let mut app = App::new();
        app.current_tab = Tab::Logs;
        app.logs.scroll_offset = 2;
        for _ in 0..30 {
            app.logs.entries.push_back(make_log_entry(
                tracing::Level::INFO,
                "opengoose::evolver",
                true,
            ));
        }
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("terminal draw should succeed");
    }

    #[test]
    fn render_chat_with_agent_busy_and_scroll_offset() {
        let mut app = App::new();
        app.agent_busy = true;
        app.chat.scroll_offset = 2;
        for i in 0..30 {
            app.chat.lines.push(ChatLine::Agent(format!("line {i}")));
        }
        let backend = TestBackend::new(80, 25);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("terminal draw should succeed");
    }
}
