use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::command;
use crate::theme;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let width = 40u16.min(area.width.saturating_sub(4));
    let height = 10u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 3;

    let palette_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, palette_area);

    let chunks = Layout::default()
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(palette_area);

    // Search input
    let input_block = Block::default()
        .title(Span::styled(" Commands ", theme::title()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));

    let input_text = format!("> {}", app.command_palette.input);
    let input = Paragraph::new(input_text)
        .style(Style::default().fg(theme::TEXT).bg(theme::SURFACE))
        .block(input_block);
    f.render_widget(input, chunks[0]);

    // Command list
    let commands = command::get_commands();
    let filtered = command::filter_commands(&commands, &app.command_palette.input);

    let lines: Vec<Line> = filtered
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let selected = i == app.command_palette.selected;
            let (prefix, style) = if selected {
                (
                    " ● ",
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("   ", Style::default().fg(theme::TEXT))
            };

            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(cmd.label, style),
            ])
        })
        .collect();

    let list_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(theme::ACCENT));

    let list = Paragraph::new(lines)
        .style(Style::default().bg(theme::SURFACE))
        .block(list_block);
    f.render_widget(list, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppMode;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_command_palette() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.command_palette.visible = true;
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_with_query() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.command_palette.visible = true;
        app.command_palette.input = "quit".into();
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_with_selection() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.command_palette.visible = true;
        app.command_palette.selected = 2;
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn test_render_small_terminal() {
        let mut app = App::new(AppMode::Normal, None, None);
        app.command_palette.visible = true;
        let backend = TestBackend::new(20, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
    }
}
