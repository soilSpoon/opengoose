use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, app: &App) {
    let ms = &app.model_select;
    if !ms.visible {
        return;
    }

    let area = f.area();

    if ms.loading {
        // Show loading indicator
        let height = 5u16.min(area.height.saturating_sub(4));
        let width = 40u16.min(area.width.saturating_sub(4));

        let vertical = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .split(area);
        let horizontal = Layout::horizontal([Constraint::Length(width)])
            .flex(Flex::Center)
            .split(vertical[0]);
        let box_area = horizontal[0];

        f.render_widget(Clear, box_area);

        let block = Block::default()
            .title(" Fetching Models ")
            .title_style(theme::title())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ACCENT));

        let content = Paragraph::new(Line::from(Span::styled("  Loading...", theme::muted())))
            .style(Style::default().bg(theme::SURFACE))
            .block(block);
        f.render_widget(content, box_area);
        return;
    }

    let models = &ms.models;
    if models.is_empty() {
        return;
    }

    // Show at most 20 models in the visible window
    let max_visible = 20usize;
    let list_height = models.len().min(max_visible) as u16 + 4;
    let height = list_height.min(area.height.saturating_sub(4));
    let width = 60u16.min(area.width.saturating_sub(4));

    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    let box_area = horizontal[0];

    f.render_widget(Clear, box_area);

    let title = format!(" Models — {} ({}) ", ms.provider_name, models.len());
    let block = Block::default()
        .title(title)
        .title_style(theme::title())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));

    // Compute visible window around selected item
    let visible_count = (height as usize).saturating_sub(3); // border + title + hint
    let scroll_offset = if ms.selected >= visible_count {
        ms.selected - visible_count + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = models
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_count)
        .map(|(i, model)| {
            let marker = if i == ms.selected { " > " } else { "   " };
            let style = if i == ms.selected {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };
            Line::from(Span::styled(format!("{marker}{model}"), style))
        })
        .collect();

    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("Esc", theme::key_hint()),
        Span::styled(": close", theme::muted()),
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
    fn test_render_model_select() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.model_select.visible = true;
        app.model_select.models = vec![
            "claude-sonnet-4-20250514".into(),
            "claude-3-5-haiku-20241022".into(),
        ];
        app.model_select.selected = 0;
        app.model_select.provider_name = "anthropic".into();

        let backend = TestBackend::new(70, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_loading() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.model_select.visible = true;
        app.model_select.loading = true;

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
    }
}
