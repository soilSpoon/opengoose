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

    let line = Line::from(vec![
        Span::styled(" OpenGoose v0.1.0", theme::title()),
        Span::raw("  "),
        Span::styled(
            format!("Sessions: {}", app.session_count),
            theme::muted(),
        ),
        Span::raw(" ".repeat(
            area.width
                .saturating_sub(40 + indicator.len() as u16 + 5) as usize,
        )),
        Span::styled("Discord: ", theme::muted()),
        Span::styled(indicator, Style::default().fg(color)),
        Span::raw(" "),
    ]);

    let bar = Paragraph::new(line).style(theme::bar());
    f.render_widget(bar, area);
}
