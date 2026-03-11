use crate::app::{App, Panel};

pub(crate) fn scroll_down(app: &mut App) {
    match app.active_panel {
        Panel::Sessions => app.select_next_session(),
        Panel::Messages => {
            let max = app
                .messages_line_count()
                .saturating_sub(app.messages_area_height);
            app.messages_scroll = app.messages_scroll.saturating_add(1).min(max);
        }
        Panel::Events => {
            let max = app
                .events_line_count()
                .saturating_sub(app.events_area_height);
            app.events_scroll = app.events_scroll.saturating_add(1).min(max);
        }
    }
}

pub(crate) fn scroll_up(app: &mut App) {
    match app.active_panel {
        Panel::Sessions => app.select_previous_session(),
        Panel::Messages => app.messages_scroll = app.messages_scroll.saturating_sub(1),
        Panel::Events => app.events_scroll = app.events_scroll.saturating_sub(1),
    }
}

pub(crate) fn scroll_to_bottom(app: &mut App) {
    match app.active_panel {
        Panel::Sessions => app.select_last_session(),
        Panel::Messages => {
            app.messages_scroll = app
                .messages_line_count()
                .saturating_sub(app.messages_area_height);
        }
        Panel::Events => {
            app.events_scroll = app
                .events_line_count()
                .saturating_sub(app.events_area_height);
        }
    }
}

pub(crate) fn scroll_to_top(app: &mut App) {
    match app.active_panel {
        Panel::Sessions => app.select_first_session(),
        Panel::Messages => app.messages_scroll = 0,
        Panel::Events => app.events_scroll = 0,
    }
}

pub(crate) fn page_up(app: &mut App) {
    match app.active_panel {
        Panel::Sessions => {
            let step = page_step(app.sessions_area_height);
            app.select_session(app.selected_session_index.saturating_sub(step));
        }
        Panel::Messages => {
            app.messages_scroll = app
                .messages_scroll
                .saturating_sub(page_step(app.messages_area_height));
        }
        Panel::Events => {
            app.events_scroll = app
                .events_scroll
                .saturating_sub(page_step(app.events_area_height));
        }
    }
}

pub(crate) fn page_down(app: &mut App) {
    match app.active_panel {
        Panel::Sessions => {
            if app.sessions.is_empty() {
                return;
            }
            let step = page_step(app.sessions_area_height);
            app.select_session((app.selected_session_index + step).min(app.sessions.len() - 1));
        }
        Panel::Messages => {
            let max = app
                .messages_line_count()
                .saturating_sub(app.messages_area_height);
            app.messages_scroll =
                (app.messages_scroll + page_step(app.messages_area_height)).min(max);
        }
        Panel::Events => {
            let max = app
                .events_line_count()
                .saturating_sub(app.events_area_height);
            app.events_scroll = (app.events_scroll + page_step(app.events_area_height)).min(max);
        }
    }
}

fn page_step(height: usize) -> usize {
    height.saturating_sub(1).max(1)
}
