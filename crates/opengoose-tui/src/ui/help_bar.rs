use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, AppMode};
use crate::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let keys: &[(&str, &str)] = match app.mode {
        AppMode::Setup => &[
            ("Enter", "Enter token"),
            ("q", "Quit"),
        ],
        AppMode::Normal => &[
            ("Ctrl+O", "Commands"),
            ("Tab", "Switch Panel"),
            ("j/k", "Scroll"),
            ("G/g", "Bottom/Top"),
            ("q", "Quit"),
        ],
    };

    let mut spans = Vec::new();
    spans.push(Span::raw(" "));
    for (i, (key, desc)) in keys.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(*key, theme::key_hint()));
        spans.push(Span::styled(format!(" {}", desc), theme::muted()));
    }

    let bar = Paragraph::new(Line::from(spans)).style(theme::bar());
    f.render_widget(bar, area);
}
