use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, AppMode};
use crate::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let keys: &[(&str, &str)] = match app.mode {
        AppMode::Setup => &[("Enter", "Enter token"), ("q", "Quit")],
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Position;

    fn row_text(terminal: &Terminal<TestBackend>, y: u16) -> String {
        let buf = terminal.backend().buffer();
        (0..buf.area.width)
            .map(|x| {
                buf.cell(Position { x, y })
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect()
    }

    #[test]
    fn test_render_normal_mode() {
        let app = App::new(AppMode::Normal, None, None);
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("Ctrl+O"));
        assert!(text.contains("Tab"));
        assert!(text.contains("Quit"));
    }

    #[test]
    fn test_render_setup_mode() {
        let app = App::new(AppMode::Setup, None, None);
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("Enter"));
        assert!(text.contains("Quit"));
        // Normal keys should NOT be present
        assert!(!text.contains("Tab"));
    }
}
