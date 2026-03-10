use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Panel};
use crate::theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Messages;
    let title = format!(
        " Compose to {} ",
        App::format_session_label(&app.composer_session_key())
    );

    let block = Block::default()
        .title(Span::styled(
            title,
            if is_active {
                theme::title()
            } else {
                theme::muted()
            },
        ))
        .borders(Borders::ALL)
        .border_style(theme::border(is_active));

    let inner = block.inner(area);
    let width = inner.width as usize;
    let offset = if width == 0 || app.composer.cursor < width {
        0
    } else {
        app.composer.cursor + 1 - width
    };

    let text = app
        .composer
        .input
        .chars()
        .skip(offset)
        .take(width)
        .collect::<String>();
    let paragraph = Paragraph::new(text).style(theme::bar()).block(block);
    f.render_widget(paragraph, area);

    if !is_active
        || inner.width == 0
        || inner.height == 0
        || app.secret_input.visible
        || app.provider_select.visible
        || app.model_select.visible
        || app.command_palette.visible
    {
        return;
    }

    let cursor_x = inner.x
        + app
            .composer
            .cursor
            .saturating_sub(offset)
            .min(width.saturating_sub(1)) as u16;
    f.set_cursor_position(Position {
        x: cursor_x,
        y: inner.y,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppMode, SessionListEntry};
    use opengoose_types::{Platform, SessionKey};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_render_sets_cursor_for_active_session() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(AppMode::Normal, None, None);
        app.sessions.push(SessionListEntry {
            session_key: SessionKey::direct(Platform::Discord, "user-1"),
            active_team: None,
            created_at: None,
            updated_at: None,
            is_active: true,
        });
        app.select_session(0);
        app.composer.input = "hello".into();
        app.composer.cursor = 5;

        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        terminal
            .backend_mut()
            .assert_cursor_position(Position { x: 6, y: 1 });
    }

    #[test]
    fn test_render_sets_cursor_for_local_composer() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(AppMode::Normal, None, None);
        app.composer.input = "hello".into();
        app.composer.cursor = 5;

        terminal.draw(|f| render(f, &app, f.area())).unwrap();
        terminal
            .backend_mut()
            .assert_cursor_position(Position { x: 6, y: 1 });
    }
}
