use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub status_bar: Rect,
    pub messages: Rect,
    pub events: Rect,
    pub help_bar: Rect,
}

pub fn create_layout(area: Rect) -> AppLayout {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(5),   // body
            Constraint::Length(1), // help bar
        ])
        .split(area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(65), // messages
            Constraint::Percentage(35), // events
        ])
        .split(vertical[1]);

    AppLayout {
        status_bar: vertical[0],
        messages: body[0],
        events: body[1],
        help_bar: vertical[2],
    }
}
