use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

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
    let teams_text = {
        let count = app.active_teams.len();
        if count > 0 {
            format!("Teams: {count} active")
        } else {
            "Teams: --".to_string()
        }
    };
    let discord_label = "Discord: ";
    let trailing = " ";

    let used: u16 = left_label.len() as u16
        + separator.len() as u16
        + sessions_text.len() as u16
        + separator.len() as u16
        + teams_text.len() as u16
        + discord_label.len() as u16
        + indicator.len() as u16
        + trailing.len() as u16;
    let padding = area.width.saturating_sub(used) as usize;

    let line = Line::from(vec![
        Span::styled(left_label, theme::title()),
        Span::raw(separator),
        Span::styled(sessions_text, theme::muted()),
        Span::raw(separator),
        Span::styled(
            teams_text,
            if app.active_teams.is_empty() {
                theme::muted()
            } else {
                Style::default().fg(theme::SUCCESS)
            },
        ),
        Span::raw(" ".repeat(padding)),
        Span::styled(discord_label, theme::muted()),
        Span::styled(indicator, Style::default().fg(color)),
        Span::raw(trailing),
    ]);

    let bar = Paragraph::new(line).style(theme::bar());
    f.render_widget(bar, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppMode;
    use opengoose_types::SessionKey;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Position;

    fn test_app() -> App {
        App::new(AppMode::Normal, None, None)
    }

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
    fn test_render_disconnected() {
        let app = test_app();
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("OpenGoose v0.1.0"));
        assert!(text.contains("Disconnected"));
        assert!(text.contains("Sessions: 0"));
    }

    #[test]
    fn test_render_connected() {
        let mut app = test_app();
        app.discord_connected = true;
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("Connected"));
    }

    #[test]
    fn test_render_with_sessions() {
        let mut app = test_app();
        app.active_sessions.insert(SessionKey::dm("user1"));
        app.active_sessions.insert(SessionKey::dm("user2"));
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("Sessions: 2"));
    }

    #[test]
    fn test_render_narrow_width() {
        let app = test_app();
        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        // Should not panic even with narrow width
    }
}
