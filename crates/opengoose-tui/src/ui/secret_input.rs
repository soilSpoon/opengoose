use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let width = 50u16.min(area.width.saturating_sub(4));
    let height = 6u16.min(area.height.saturating_sub(4));

    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    let box_area = horizontal[0];

    f.render_widget(Clear, box_area);

    let block = Block::default()
        .title(" Discord Bot Token ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));

    // Mask input with bullets
    let masked: String = "\u{2022}".repeat(app.secret_input.input.len());
    let inner_width = width.saturating_sub(6) as usize;
    let char_count = masked.chars().count();
    let display: String = if char_count > inner_width {
        masked.chars().skip(char_count - inner_width).collect()
    } else {
        masked
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(" > ", theme::key_hint()),
        Span::styled(display, Style::default().fg(theme::TEXT)),
        Span::styled("_", theme::subtle()),
    ])];

    if let Some(ref msg) = app.secret_input.status_message {
        lines.push(Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(theme::ERROR),
        )));
    }

    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("Enter", theme::key_hint()),
        Span::styled(": save  ", theme::muted()),
        Span::styled("Esc", theme::key_hint()),
        Span::styled(": cancel", theme::muted()),
    ]));

    let paragraph = Paragraph::new(lines)
        .style(Style::default().bg(theme::SURFACE))
        .block(block);
    f.render_widget(paragraph, box_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppMode;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_empty_input() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.secret_input.visible = true;
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_with_input() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.secret_input.visible = true;
        app.secret_input.input = "my_secret_token".into();
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_with_status_message() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.secret_input.visible = true;
        app.secret_input.status_message = Some("Token cannot be empty".into());
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_long_input_scrolls() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.secret_input.visible = true;
        app.secret_input.input = "x".repeat(100);
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_small_terminal() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.secret_input.visible = true;
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }
}
