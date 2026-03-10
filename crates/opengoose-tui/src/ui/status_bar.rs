use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use opengoose_types::Platform;

use crate::app::{AgentStatus, App, EventLevel};
use crate::theme;

const ALL_PLATFORMS: [Platform; 3] = [Platform::Discord, Platform::Telegram, Platform::Slack];

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let sessions_text = format!("Sessions: {}", app.sessions.len());
    let agent_text = match app.agent_status {
        AgentStatus::Idle => ("Agent: idle", theme::TEXT_MUTED),
        AgentStatus::Thinking => ("Agent: thinking", theme::SECONDARY),
        AgentStatus::Generating => ("Agent: generating", theme::SUCCESS),
    };

    let mut spans = vec![
        Span::styled(" OpenGoose v0.1.0", theme::title()),
        Span::raw("  "),
        Span::styled(sessions_text, theme::muted()),
        Span::raw("  "),
        Span::styled(agent_text.0, Style::default().fg(agent_text.1)),
    ];

    if let Some(notice) = &app.status_notice {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            truncate_notice(&notice.message, area.width.saturating_sub(24) as usize),
            Style::default().fg(match notice.level {
                EventLevel::Info => theme::SECONDARY,
                EventLevel::Error => theme::ERROR,
            }),
        ));
    }

    spans.push(Span::raw("  "));
    for (index, platform) in ALL_PLATFORMS.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let connected = app.connected_platforms.contains(platform);
        spans.push(Span::styled(
            format!(
                "{} {}",
                if connected { "●" } else { "○" },
                platform.as_str()
            ),
            Style::default().fg(if connected {
                theme::SUCCESS
            } else {
                theme::TEXT_MUTED
            }),
        ));
    }

    let bar = Paragraph::new(Line::from(spans)).style(theme::bar());
    f.render_widget(bar, area);
}

fn truncate_notice(message: &str, max_chars: usize) -> String {
    if max_chars == 0 || message.chars().count() <= max_chars {
        return message.to_string();
    }

    message
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>()
        + "..."
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
    fn test_render_idle_status() {
        let app = test_app();
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("OpenGoose v0.1.0"));
        assert!(text.contains("Sessions: 0"));
        assert!(text.contains("Agent: idle"));
    }

    #[test]
    fn test_render_generating_status() {
        let mut app = test_app();
        app.agent_status = AgentStatus::Generating;
        app.connected_platforms.insert(Platform::Discord);
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("Agent: generating"));
        assert!(text.contains("● discord"));
    }

    #[test]
    fn test_render_notice() {
        let mut app = test_app();
        app.status_notice = Some(crate::app::StatusNotice {
            message: "Connection timed out. Retrying soon.".into(),
            level: EventLevel::Error,
        });
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        let text = row_text(&terminal, 0);
        assert!(text.contains("Connection timed out"));
    }
}
