use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, MessageEntry, Panel};
use crate::theme;

fn author_color(author: &str) -> ratatui::style::Color {
    if author == "goose" {
        theme::SUCCESS
    } else {
        theme::ACCENT
    }
}

fn split_long_word(word: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let chars = word.chars().collect::<Vec<_>>();
    for chunk in chars.chunks(width) {
        chunks.push(chunk.iter().collect());
    }
    chunks
}

fn wrap_segment(segment: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    if segment.trim().is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in segment.split_whitespace() {
        let word_width = word.chars().count();
        if current.is_empty() {
            if word_width <= width {
                current.push_str(word);
                continue;
            }

            lines.extend(split_long_word(word, width));
            continue;
        }

        let current_width = current.chars().count();
        if current_width + 1 + word_width <= width {
            current.push(' ');
            current.push_str(word);
            continue;
        }

        lines.push(std::mem::take(&mut current));
        if word_width <= width {
            current.push_str(word);
        } else {
            let mut chunks = split_long_word(word, width);
            if let Some(last) = chunks.pop() {
                lines.extend(chunks);
                current = last;
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn wrap_text(content: &str, width: usize) -> Vec<String> {
    let normalized = content.replace('\r', "");
    let mut lines = Vec::new();

    for segment in normalized.split('\n') {
        lines.extend(wrap_segment(segment, width));
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn render_message_lines(message: &MessageEntry, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }

    let prefix = format!("[{}] ", message.author);
    let prefix_width = prefix.chars().count();
    let body_style = Style::default().fg(theme::TEXT);
    let prefix_style = Style::default().fg(author_color(&message.author));
    let content = message.content.replace('\r', "");

    if width <= prefix_width + 1 {
        let mut lines = vec![Line::from(Span::styled(
            prefix.trim_end().to_string(),
            prefix_style,
        ))];
        if content.is_empty() {
            return lines;
        }
        for segment in wrap_text(&content, width.saturating_sub(2).max(1)) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(segment, body_style),
            ]));
        }
        return lines;
    }

    let wrapped = wrap_text(&content, width.saturating_sub(prefix_width).max(1));
    let indent = " ".repeat(prefix_width);

    let mut lines = Vec::new();
    if let Some(first) = wrapped.first() {
        lines.push(Line::from(vec![
            Span::styled(prefix.clone(), prefix_style),
            Span::styled(first.clone(), body_style),
        ]));
    }

    for segment in wrapped.into_iter().skip(1) {
        lines.push(Line::from(vec![
            Span::raw(indent.clone()),
            Span::styled(segment, body_style),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            prefix.trim_end().to_string(),
            prefix_style,
        )));
    }

    lines
}

fn rendered_lines_for_width(app: &App, width: usize) -> Vec<Line<'static>> {
    if app.messages.is_empty() {
        return vec![Line::default()];
    }

    let mut lines = Vec::new();
    for (index, message) in app.messages.iter().enumerate() {
        lines.extend(render_message_lines(message, width.max(1)));
        if index + 1 < app.messages.len() {
            lines.push(Line::default());
        }
    }

    if lines.is_empty() {
        lines.push(Line::default());
    }

    lines
}

pub fn total_content_height(app: &App) -> usize {
    total_content_height_for_width(app, app.messages_area_width as u16)
}

pub fn total_content_height_for_width(app: &App, width: u16) -> usize {
    rendered_lines_for_width(app, width as usize).len().max(1)
}

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Messages;

    let outer_block = Block::default()
        .title(Span::styled(
            format!(" Conversation ({}) ", app.messages.len()),
            if is_active {
                theme::title()
            } else {
                theme::muted()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme::border(is_active));

    if app.messages.is_empty() {
        let empty_text = if app.selected_session.is_some() {
            "  No conversation history loaded for this session yet."
        } else {
            "  Start typing below to begin a local conversation."
        };
        let empty = Paragraph::new(empty_text)
            .style(theme::muted())
            .block(outer_block);
        f.render_widget(empty, area);
        return;
    }

    let inner = outer_block.inner(area);
    f.render_widget(outer_block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let lines = rendered_lines_for_width(app, inner.width as usize);
    let paragraph = Paragraph::new(lines).scroll((app.messages_scroll as u16, 0));
    f.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, AppMode};
    use opengoose_types::{Platform, SessionKey};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn test_app() -> App {
        let mut app = App::new(AppMode::Normal, None, None);
        app.messages_area_width = 24;
        app
    }

    fn add_msg(app: &mut App, author: &str, content: &str) {
        app.messages.push_back(MessageEntry {
            session_key: SessionKey::dm(Platform::Discord, "user-1"),
            author: author.into(),
            content: content.into(),
        });
    }

    #[test]
    fn test_total_content_height_empty() {
        let app = test_app();
        assert_eq!(total_content_height(&app), 1);
    }

    #[test]
    fn test_total_content_height_includes_message_gap() {
        let mut app = test_app();
        add_msg(&mut app, "alice", "hello");
        add_msg(&mut app, "goose", "world");

        assert_eq!(total_content_height(&app), 3);
    }

    #[test]
    fn test_total_content_height_counts_wrapped_lines() {
        let mut app = test_app();
        app.messages_area_width = 12;
        add_msg(
            &mut app,
            "alice",
            "this is a much longer message that should wrap across lines",
        );

        assert!(total_content_height(&app) > 3);
    }

    #[test]
    fn test_total_content_height_preserves_newlines() {
        let mut app = test_app();
        add_msg(&mut app, "alice", "first line\nsecond line");

        assert_eq!(total_content_height(&app), 2);
    }

    #[test]
    fn test_render_empty_messages() {
        let app = test_app();
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_wrapped_message() {
        let mut app = test_app();
        add_msg(
            &mut app,
            "alice",
            "line one is long enough to wrap onto the next line in the panel",
        );
        let backend = TestBackend::new(30, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_with_scroll() {
        let mut app = test_app();
        app.messages_area_width = 16;
        app.messages_scroll = 2;
        add_msg(&mut app, "alice", "one two three four five six seven eight");
        add_msg(
            &mut app,
            "goose",
            "nine ten eleven twelve thirteen fourteen",
        );
        let backend = TestBackend::new(28, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }
}
