use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Panel};
use crate::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Sessions;

    let block = Block::default()
        .title(Span::styled(
            format!(" Sessions ({}) ", app.sessions.len()),
            if is_active {
                theme::title()
            } else {
                theme::muted()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme::border(is_active));

    if app.sessions.is_empty() {
        let empty = Paragraph::new("  No sessions yet. Press Ctrl+N to start one.")
            .style(theme::muted())
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let lines = app
        .sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let selected = index == app.selected_session_index;
            let status_color = if session.is_active {
                theme::SUCCESS
            } else {
                theme::TEXT_MUTED
            };
            let label_style = if selected {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT)
            };

            let mut spans = vec![
                Span::styled(
                    if selected { ">" } else { " " },
                    Style::default().fg(theme::ACCENT),
                ),
                Span::raw(" "),
                Span::styled(if session.is_active { "●" } else { "○" }, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(App::format_session_label(&session.session_key), label_style),
            ];

            if let Some(team) = &session.active_team {
                spans.push(Span::styled(
                    format!(" [{team}]"),
                    Style::default().fg(theme::SECONDARY),
                ));
            }

            Line::from(spans)
        })
        .collect::<Vec<_>>();

    let para = Paragraph::new(lines)
        .block(block)
        .scroll((app.sessions_scroll as u16, 0));
    f.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppMode, SessionListEntry};
    use opengoose_types::{Platform, SessionKey};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn test_app() -> App {
        App::new(AppMode::Normal, None, None)
    }

    #[test]
    fn test_render_empty_sessions() {
        let app = test_app();
        let backend = TestBackend::new(30, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_render_sessions_list() {
        let mut app = test_app();
        app.sessions.push(SessionListEntry {
            session_key: SessionKey::direct(Platform::Discord, "user-1"),
            active_team: Some("triage".into()),
            created_at: None,
            updated_at: None,
            is_active: true,
        });
        app.select_session(0);

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app, f.area())).unwrap();
    }
}
