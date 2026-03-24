use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthChar;

use super::super::app::{App, ChatLine};

pub fn render_chat(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize; // borders

    let lines: Vec<Line> = app.chat.lines.iter().flat_map(chat_line_to_lines).collect();

    let total = lines.len();
    let skip = if app.chat.scroll_offset == 0 {
        total.saturating_sub(inner_height)
    } else {
        total.saturating_sub(inner_height + app.chat.scroll_offset)
    };

    let visible: Vec<Line> = lines.into_iter().skip(skip).collect();

    let busy_indicator = if app.agent_busy { " ⏳" } else { "" };
    let title = format!(" Chat{busy_indicator} ");

    let paragraph = Paragraph::new(visible)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

pub fn chat_line_to_lines(cl: &ChatLine) -> Vec<Line<'_>> {
    match cl {
        ChatLine::User(text) => vec![Line::from(vec![
            Span::styled(
                "> ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(text.as_str(), Style::default().fg(Color::Cyan)),
        ])],
        ChatLine::Agent(text) => text
            .lines()
            .map(|line| Line::from(Span::styled(line, Style::default().fg(Color::White))))
            .collect(),
        ChatLine::System(text) => vec![Line::from(Span::styled(
            text.as_str(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::ITALIC),
        ))],
    }
}

pub fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let paragraph = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Cyan)),
        Span::raw(&app.chat.input),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(paragraph, area);

    let display_width: u16 = app
        .chat
        .input
        .chars()
        .take(app.chat.cursor_pos)
        .map(|c: char| c.width().unwrap_or(0) as u16)
        .sum();
    frame.set_cursor_position((area.x + 3 + display_width, area.y + 1));
}
