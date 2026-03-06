use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use opengoose_types::Platform;

use crate::app::App;
use crate::theme;

/// All platforms in display order.
const ALL_PLATFORMS: [Platform; 3] = [Platform::Discord, Platform::Telegram, Platform::Slack];

pub fn render(f: &mut Frame, app: &App, area: Rect) {
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

    // Build channel status spans: [discord] [telegram] [slack]
    let mut channel_spans: Vec<Span> = Vec::new();
    for platform in &ALL_PLATFORMS {
        let connected = app.connected_platforms.contains(platform);
        let (icon, color) = if connected {
            ("●", theme::SUCCESS)
        } else {
            ("○", theme::TEXT_MUTED)
        };
        if !channel_spans.is_empty() {
            channel_spans.push(Span::raw(" "));
        }
        channel_spans.push(Span::styled(
            format!("{icon} {}", platform.as_str()),
            Style::default().fg(color),
        ));
    }

    // Calculate remaining width for padding
    let channel_text_len: usize = ALL_PLATFORMS
        .iter()
        .map(|p| 2 + p.as_str().len()) // "● " + platform name
        .sum::<usize>()
        + (ALL_PLATFORMS.len() - 1); // spaces between

    let trailing = " ";
    let used: u16 = left_label.len() as u16
        + separator.len() as u16
        + sessions_text.len() as u16
        + separator.len() as u16
        + teams_text.len() as u16
        + separator.len() as u16
        + channel_text_len as u16
        + trailing.len() as u16;
    let padding = area.width.saturating_sub(used) as usize;

    let mut spans = vec![
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
    ];
    spans.extend(channel_spans);
    spans.push(Span::raw(trailing));

    let line = Line::from(spans);
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
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("OpenGoose v0.1.0"));
        assert!(text.contains("Sessions: 0"));
        // All platforms should show as disconnected (○)
        assert!(text.contains("○ discord"));
        assert!(text.contains("○ telegram"));
        assert!(text.contains("○ slack"));
    }

    #[test]
    fn test_render_connected() {
        let mut app = test_app();
        app.connected_platforms.insert(Platform::Discord);
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        // Discord should show connected (●), others disconnected (○)
        assert!(text.contains("● discord"));
        assert!(text.contains("○ telegram"));
        assert!(text.contains("○ slack"));
    }

    #[test]
    fn test_render_multi_connected() {
        let mut app = test_app();
        app.connected_platforms.insert(Platform::Discord);
        app.connected_platforms.insert(Platform::Telegram);
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("● discord"));
        assert!(text.contains("● telegram"));
        assert!(text.contains("○ slack"));
    }

    #[test]
    fn test_render_with_sessions() {
        let mut app = test_app();
        app.active_sessions
            .insert(SessionKey::dm(Platform::Discord, "user1"));
        app.active_sessions
            .insert(SessionKey::dm(Platform::Discord, "user2"));
        let backend = TestBackend::new(100, 1);
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
