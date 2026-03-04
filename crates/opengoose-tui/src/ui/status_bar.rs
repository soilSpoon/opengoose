use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let (indicator, color) = if app.discord_connected {
        ("● Connected", theme::SUCCESS)
    } else {
        ("○ Disconnected", theme::ERROR)
    };

    let left_label = " OpenGoose v0.1.0";
    let separator = "  ";
    let sessions_text = format!("Sessions: {}", app.active_sessions.len());
    let discord_label = "Discord: ";
    let trailing = " ";

    let used: u16 = left_label.len() as u16
        + separator.len() as u16
        + sessions_text.len() as u16
        + discord_label.len() as u16
        + indicator.len() as u16
        + trailing.len() as u16;
    let padding = area.width.saturating_sub(used) as usize;

    let line = Line::from(vec![
        Span::styled(left_label, theme::title()),
        Span::raw(separator),
        Span::styled(sessions_text, theme::muted()),
        Span::raw(" ".repeat(padding)),
        Span::styled(discord_label, theme::muted()),
        Span::styled(indicator, Style::default().fg(color)),
        Span::raw(trailing),
    ]);

    let bar = Paragraph::new(line).style(theme::bar());
    f.render_widget(bar, area);
}
