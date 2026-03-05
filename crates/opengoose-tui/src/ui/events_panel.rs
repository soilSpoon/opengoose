use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, EventLevel, Panel};
use crate::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Events;

    let block = Block::default()
        .title(Span::styled(
            format!(" Events ({}) ", app.events.len()),
            if is_active {
                theme::title()
            } else {
                theme::muted()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme::border(is_active));

    if app.events.is_empty() {
        let empty = Paragraph::new("  No events yet...")
            .style(theme::muted())
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let lines: Vec<Line> = app
        .events
        .iter()
        .map(|evt| {
            let elapsed = evt.timestamp.duration_since(app.start_time);
            let secs = elapsed.as_secs();
            let mins = secs / 60;
            let secs = secs % 60;
            let time_str = format!("{:02}:{:02}", mins, secs);

            let color = match evt.level {
                EventLevel::Info => theme::TEXT,
                EventLevel::Error => theme::ERROR,
            };

            Line::from(vec![
                Span::styled(format!(" {} ", time_str), theme::subtle()),
                Span::styled(&evt.summary, Style::default().fg(color)),
            ])
        })
        .collect();

    let scroll = app.events_scroll as u16;
    let para = Paragraph::new(lines).block(block).scroll((scroll, 0));
    f.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppMode;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Position;

    fn test_app() -> App {
        App::new(AppMode::Normal, None, None)
    }

    #[test]
    fn test_render_empty_events() {
        let app = test_app();
        let backend = TestBackend::new(40, 10);
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
        assert!(text.contains("No events yet"));
    }

    #[test]
    fn test_render_with_info_events() {
        let mut app = test_app();
        app.push_event("test info event", EventLevel::Info);
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer().clone();
        // Should contain the event summary
        let row: String = (0..buf.area.width)
            .map(|x| {
                buf.cell(Position { x, y: 1 })
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(row.contains("test info event"));
    }

    #[test]
    fn test_render_with_error_events() {
        let mut app = test_app();
        app.push_event("bad error", EventLevel::Error);
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_active_panel() {
        let mut app = test_app();
        app.active_panel = Panel::Events;
        app.push_event("active", EventLevel::Info);
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_with_scroll() {
        let mut app = test_app();
        for i in 0..20 {
            app.push_event(&format!("event {i}"), EventLevel::Info);
        }
        app.events_scroll = 5;
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_timestamp_format() {
        let mut app = test_app();
        app.push_event("timed", EventLevel::Info);
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer().clone();
        let row: String = (0..buf.area.width)
            .map(|x| {
                buf.cell(Position { x, y: 1 })
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        // Timestamp should be "00:00" format
        assert!(row.contains("00:0"));
    }
}
