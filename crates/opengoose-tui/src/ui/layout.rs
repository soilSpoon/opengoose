use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub status_bar: Rect,
    pub sessions: Rect,
    pub messages: Rect,
    pub events: Rect,
    pub composer: Rect,
    pub help_bar: Rect,
}

pub fn create_layout(area: Rect) -> AppLayout {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(5),    // body
            Constraint::Length(3), // composer
            Constraint::Length(1), // help bar
        ])
        .split(area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(24), // sessions
            Constraint::Percentage(46), // messages
            Constraint::Percentage(30), // events
        ])
        .split(vertical[1]);

    AppLayout {
        status_bar: vertical[0],
        sessions: body[0],
        messages: body[1],
        events: body[2],
        composer: vertical[2],
        help_bar: vertical[3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_layout_dimensions() {
        let area = Rect::new(0, 0, 100, 40);
        let layout = create_layout(area);

        assert_eq!(layout.status_bar.height, 1);
        assert_eq!(layout.status_bar.y, 0);
        assert_eq!(layout.composer.height, 3);
        assert_eq!(layout.help_bar.height, 1);
        assert_eq!(layout.help_bar.y, 39);
        assert_eq!(layout.sessions.height, 35);
        assert_eq!(layout.messages.height, 35);
        assert_eq!(layout.events.height, 35);
        assert!(layout.messages.width > layout.events.width);
        assert_eq!(
            layout.sessions.width + layout.messages.width + layout.events.width,
            100
        );
    }

    #[test]
    fn test_create_layout_small_area() {
        let area = Rect::new(0, 0, 20, 10);
        let layout = create_layout(area);
        assert_eq!(layout.status_bar.height, 1);
        assert_eq!(layout.composer.height, 3);
        assert_eq!(layout.help_bar.height, 1);
        assert_eq!(layout.messages.height, 5);
    }
}
