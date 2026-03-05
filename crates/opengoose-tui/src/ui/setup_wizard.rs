use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::theme;

pub fn render(f: &mut Frame) {
    let area = f.area();

    let width = 50u16.min(area.width.saturating_sub(4));
    let height = 11u16.min(area.height.saturating_sub(4));

    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    let box_area = horizontal[0];

    f.render_widget(Clear, box_area);

    let block = Block::default()
        .title(" OpenGoose Setup ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Welcome to OpenGoose!",
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  A Discord bot token is required.",
            theme::muted(),
        )),
        Line::from(Span::styled(
            "  https://discord.com/developers",
            Style::default().fg(theme::SECONDARY),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Enter", theme::key_hint()),
            Span::styled(": Enter token  ", theme::muted()),
            Span::styled("q", theme::key_hint()),
            Span::styled(": Quit", theme::muted()),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .style(Style::default().bg(theme::SURFACE))
        .block(block);
    f.render_widget(paragraph, box_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Position;
    use ratatui::Terminal;

    #[test]
    fn test_render_wizard() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f)).unwrap();
        // Verify key content is rendered
        let buf = terminal.backend().buffer().clone();
        let mut all_text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                if let Some(cell) = buf.cell(Position { x, y }) {
                    all_text.push(cell.symbol().chars().next().unwrap_or(' '));
                }
            }
        }
        assert!(all_text.contains("Welcome to OpenGoose"));
        assert!(all_text.contains("Discord bot token"));
    }

    #[test]
    fn test_render_small_terminal() {
        let backend = TestBackend::new(20, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f)).unwrap();
    }
}
