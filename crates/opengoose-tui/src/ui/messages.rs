use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::{App, MessageEntry, Panel};
use crate::theme;

struct SessionGroup<'a> {
    label: String,
    messages: Vec<&'a MessageEntry>,
}

fn group_messages_by_session(app: &App) -> Vec<SessionGroup<'_>> {
    let mut groups: Vec<SessionGroup<'_>> = Vec::new();
    for msg in app.messages.iter() {
        let label = msg.session_key.to_string();
        if groups.last().is_none_or(|g| g.label != label) {
            groups.push(SessionGroup {
                label,
                messages: vec![msg],
            });
        } else {
            groups.last_mut().unwrap().messages.push(msg);
        }
    }
    groups
}

/// Total height of all session groups (Block borders + content + gaps).
pub fn total_content_height(app: &App) -> usize {
    if app.messages.is_empty() {
        return 1;
    }
    let groups = group_messages_by_session(app);
    let mut h: usize = 0;
    for (i, g) in groups.iter().enumerate() {
        h += g.messages.len() + 2; // +2 for top/bottom block borders
        if i < groups.len() - 1 {
            h += 1; // gap between groups
        }
    }
    h
}

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Messages;

    let outer_block = Block::default()
        .title(Span::styled(
            format!(" Messages ({}) ", app.messages.len()),
            if is_active {
                theme::title()
            } else {
                theme::muted()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme::border(is_active));

    if app.messages.is_empty() {
        let empty = Paragraph::new("  No messages yet. Waiting for Discord messages...")
            .style(theme::muted())
            .block(outer_block);
        f.render_widget(empty, area);
        return;
    }

    let inner = outer_block.inner(area);
    f.render_widget(outer_block, area);

    let groups = group_messages_by_session(app);

    // Calculate total height
    let total_height = total_content_height(app);
    if total_height == 0 || inner.width == 0 || inner.height == 0 {
        return;
    }

    // Render all session groups into a temporary buffer
    let mut temp_buf = Buffer::empty(Rect {
        x: 0,
        y: 0,
        width: inner.width,
        height: total_height as u16,
    });

    let mut y: u16 = 0;
    for (i, group) in groups.iter().enumerate() {
        let group_height = group.messages.len() as u16 + 2;
        let group_rect = Rect {
            x: 0,
            y,
            width: inner.width,
            height: group_height,
        };

        let session_block = Block::default()
            .title(Span::styled(
                format!(" {} ", group.label),
                Style::default()
                    .fg(theme::SECONDARY)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::SECONDARY));

        let lines: Vec<Line> = group
            .messages
            .iter()
            .map(|msg| {
                let author_color = if msg.author == "goose" {
                    theme::SUCCESS
                } else {
                    theme::ACCENT
                };
                // Strip Discord markdown and collapse newlines for TUI
                let plain = msg.content.replace("**", "").replace('\n', " ");
                let content = if plain.chars().count() > 120 {
                    format!("{}...", plain.chars().take(117).collect::<String>())
                } else {
                    plain
                };
                Line::from(vec![
                    Span::styled(
                        format!("[{}]", msg.author),
                        Style::default().fg(author_color),
                    ),
                    Span::styled(format!(" {}", content), Style::default().fg(theme::TEXT)),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(session_block);
        para.render(group_rect, &mut temp_buf);

        y += group_height;
        if i < groups.len() - 1 {
            y += 1; // gap between groups
        }
    }

    // Copy visible portion from temp buffer into the frame
    let scroll = app.messages_scroll;
    let frame_buf = f.buffer_mut();
    for dy in 0..inner.height {
        let src_y = scroll + dy as usize;
        if src_y >= total_height {
            break;
        }
        for dx in 0..inner.width {
            if let Some(src_cell) = temp_buf.cell(Position {
                x: dx,
                y: src_y as u16,
            }) && let Some(dst_cell) = frame_buf.cell_mut(Position {
                x: inner.x + dx,
                y: inner.y + dy,
            }) {
                *dst_cell = src_cell.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, AppMode};
    use opengoose_types::{Platform, SessionKey};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn test_app() -> App {
        App::new(AppMode::Normal, None, None)
    }

    fn add_msg(app: &mut App, session: &str, author: &str, content: &str) {
        app.messages.push_back(MessageEntry {
            session_key: SessionKey::dm(Platform::Discord, session),
            author: author.into(),
            content: content.into(),
        });
    }

    #[test]
    fn test_group_messages_single_session() {
        let mut app = test_app();
        add_msg(&mut app, "user1", "alice", "hello");
        add_msg(&mut app, "user1", "goose", "hi");
        let groups = group_messages_by_session(&app);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].messages.len(), 2);
    }

    #[test]
    fn test_group_messages_multiple_sessions() {
        let mut app = test_app();
        add_msg(&mut app, "user1", "alice", "hello");
        add_msg(&mut app, "user2", "bob", "hey");
        add_msg(&mut app, "user2", "goose", "reply");
        let groups = group_messages_by_session(&app);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].messages.len(), 1);
        assert_eq!(groups[1].messages.len(), 2);
    }

    #[test]
    fn test_group_messages_alternating_sessions() {
        let mut app = test_app();
        add_msg(&mut app, "user1", "alice", "a");
        add_msg(&mut app, "user2", "bob", "b");
        add_msg(&mut app, "user1", "alice", "c");
        let groups = group_messages_by_session(&app);
        assert_eq!(groups.len(), 3); // each change creates a new group
    }

    #[test]
    fn test_total_content_height_empty() {
        let app = test_app();
        assert_eq!(total_content_height(&app), 1);
    }

    #[test]
    fn test_total_content_height_one_group() {
        let mut app = test_app();
        add_msg(&mut app, "user1", "alice", "hello");
        add_msg(&mut app, "user1", "goose", "hi");
        // 2 messages + 2 borders = 4
        assert_eq!(total_content_height(&app), 4);
    }

    #[test]
    fn test_total_content_height_two_groups() {
        let mut app = test_app();
        add_msg(&mut app, "user1", "alice", "hello");
        add_msg(&mut app, "user2", "bob", "hey");
        // group1: 1 msg + 2 borders = 3
        // gap: 1
        // group2: 1 msg + 2 borders = 3
        // total: 7
        assert_eq!(total_content_height(&app), 7);
    }

    #[test]
    fn test_render_empty_messages() {
        let app = test_app();
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer().clone();
        let text: String = (0..buf.area.width)
            .map(|x| {
                buf.cell(Position { x, y: 1 })
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(text.contains("No messages yet"));
    }

    #[test]
    fn test_render_with_messages() {
        let mut app = test_app();
        add_msg(&mut app, "user1", "alice", "hello world");
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        // Just verify it doesn't panic and renders something
        let buf = terminal.backend().buffer().clone();
        assert!(buf.area.width > 0);
    }

    #[test]
    fn test_render_with_long_message_truncated() {
        let mut app = test_app();
        let long_msg = "x".repeat(200);
        add_msg(&mut app, "user1", "alice", &long_msg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        // Should not panic
    }

    #[test]
    fn test_render_with_goose_author() {
        let mut app = test_app();
        add_msg(&mut app, "user1", "goose", "response");
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_with_markdown_content() {
        let mut app = test_app();
        add_msg(&mut app, "user1", "alice", "**bold** text\nnewline");
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_with_scroll() {
        let mut app = test_app();
        for i in 0..20 {
            add_msg(&mut app, &format!("user{i}"), "alice", &format!("msg {i}"));
        }
        app.messages_scroll = 5;
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_inactive_panel() {
        let mut app = test_app();
        app.active_panel = Panel::Events;
        add_msg(&mut app, "user1", "alice", "hello");
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }
}
