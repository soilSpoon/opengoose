use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, EventLevel, Panel};
use crate::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Events;

    let block = Block::default()
        .title(Span::styled(
            format!(" Events ({}) ", app.events.len()),
            if is_active { theme::title() } else { theme::muted() },
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
