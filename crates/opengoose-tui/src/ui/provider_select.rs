use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, app: &App) {
    let ps = &app.provider_select;
    if !ps.visible {
        return;
    }

    let area = f.area();
    let providers = &ps.providers;

    // Compute box size based on provider count
    let list_height = providers.len() as u16 + 4; // 2 border + title + hint line
    let height = list_height.min(area.height.saturating_sub(4));
    let width = 50u16.min(area.width.saturating_sub(4));

    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    let box_area = horizontal[0];

    f.render_widget(Clear, box_area);

    let block = Block::default()
        .title(" Select Provider ")
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));

    let mut lines: Vec<Line> = providers
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let marker = if i == ps.selected { " > " } else { "   " };
            let style = if i == ps.selected {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };
            Line::from(Span::styled(format!("{marker}{p}"), style))
        })
        .collect();

    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("Enter", theme::key_hint()),
        Span::styled(": select  ", theme::muted()),
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
    fn test_render_provider_select() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.provider_select.visible = true;
        app.provider_select.providers =
            vec!["Anthropic".into(), "OpenAI".into(), "Google Gemini".into()];
        app.provider_select.selected = 1;

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_hidden() {
        let app = App::new(AppMode::Normal, None, None);
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        // Should not panic when not visible
    }
}
