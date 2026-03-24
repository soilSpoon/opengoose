use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::super::app::App;
use super::super::log_entry::LogEntry;

pub fn render_logs(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let visible = app.visible_logs();
    let total = visible.len();

    let skip = if app.logs.scroll_offset == 0 {
        total.saturating_sub(inner_height)
    } else {
        total.saturating_sub(inner_height + app.logs.scroll_offset)
    };

    let lines: Vec<Line> = visible
        .into_iter()
        .skip(skip)
        .take(inner_height)
        .map(|entry| format_log_entry(entry, app.logs.verbose))
        .collect();

    let mode_label = if app.logs.verbose {
        "verbose"
    } else {
        "structured"
    };
    let title = format!(" Logs ({mode_label}) — press v to toggle ");

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(paragraph, area);
}

pub fn format_log_entry(entry: &LogEntry, verbose: bool) -> Line<'static> {
    let time = entry.timestamp.format("%H:%M:%S").to_string();

    if verbose {
        let level_style = match entry.level {
            tracing::Level::ERROR => Style::default().fg(Color::Red),
            tracing::Level::WARN => Style::default().fg(Color::Yellow),
            tracing::Level::INFO => Style::default().fg(Color::Green),
            _ => Style::default().fg(Color::DarkGray),
        };

        Line::from(vec![
            Span::styled(time, Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(format!("{:<5}", entry.level), level_style),
            Span::raw(" "),
            Span::styled(entry.target.clone(), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::raw(entry.message.clone()),
        ])
    } else {
        let source = if entry.target.contains("::rig") {
            "worker"
        } else if entry.target.contains("evolver") {
            "evolver"
        } else {
            "system"
        };

        Line::from(vec![
            Span::styled(time, Style::default().fg(Color::DarkGray)),
            Span::styled(format!(" [{source}] "), Style::default().fg(Color::Cyan)),
            Span::raw(entry.message.clone()),
        ])
    }
}
