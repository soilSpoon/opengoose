use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use ratatui::Frame;

use crate::app::{App, MessageEntry, Panel};
use crate::theme;

struct SessionGroup<'a> {
    label: String,
    messages: Vec<&'a MessageEntry>,
}

fn group_messages_by_session(app: &App) -> Vec<SessionGroup<'_>> {
    let mut groups: Vec<SessionGroup<'_>> = Vec::new();
    for msg in app.messages.iter() {
        let label = msg.session_key.to_string();
        if groups.last().map_or(true, |g| g.label != label) {
            groups.push(SessionGroup {
                label,
                messages: vec![msg],
            });
        } else {
            groups.last_mut().unwrap().messages.push(msg);
        }
    }
    groups
}

/// Total height of all session groups (Block borders + content + gaps).
pub fn total_content_height(app: &App) -> u16 {
    if app.messages.is_empty() {
        return 1;
    }
    let groups = group_messages_by_session(app);
    let mut h: u16 = 0;
    for (i, g) in groups.iter().enumerate() {
        h += g.messages.len() as u16 + 2; // +2 for top/bottom block borders
        if i < groups.len() - 1 {
            h += 1; // gap between groups
        }
    }
    h
}

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Messages;

    let outer_block = Block::default()
        .title(Span::styled(
            format!(" Messages ({}) ", app.messages.len()),
            if is_active { theme::title() } else { theme::muted() },
        ))
        .borders(Borders::ALL)
        .border_style(theme::border(is_active));

    if app.messages.is_empty() {
        let empty = Paragraph::new("  No messages yet. Waiting for Discord messages...")
            .style(theme::muted())
            .block(outer_block);
        f.render_widget(empty, area);
        return;
    }

    let inner = outer_block.inner(area);
    f.render_widget(outer_block, area);

    let groups = group_messages_by_session(app);

    // Calculate total height
    let total_height = total_content_height(app);
    if total_height == 0 || inner.width == 0 || inner.height == 0 {
        return;
    }

    // Render all session groups into a temporary buffer
    let mut temp_buf = Buffer::empty(Rect {
        x: 0,
        y: 0,
        width: inner.width,
        height: total_height,
    });

    let mut y: u16 = 0;
    for (i, group) in groups.iter().enumerate() {
        let group_height = group.messages.len() as u16 + 2;
        let group_rect = Rect {
            x: 0,
            y,
            width: inner.width,
            height: group_height,
        };

        let session_block = Block::default()
            .title(Span::styled(
                format!(" {} ", group.label),
                Style::default()
                    .fg(theme::SECONDARY)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::SECONDARY));

        let lines: Vec<Line> = group
            .messages
            .iter()
            .map(|msg| {
                let author_color = if msg.author == "goose" {
                    theme::SUCCESS
                } else {
                    theme::ACCENT
                };
                // Strip Discord markdown and collapse newlines for TUI
                let plain = msg.content.replace("**", "").replace('\n', " ");
                let content = if plain.len() > 120 {
                    format!("{}...", &plain[..117])
                } else {
                    plain
                };
                Line::from(vec![
                    Span::styled(
                        format!("[{}]", msg.author),
                        Style::default().fg(author_color),
                    ),
                    Span::styled(
                        format!(" {}", content),
                        Style::default().fg(theme::TEXT),
                    ),
                ])
            })
            .collect();

        let para = Paragraph::new(lines).block(session_block);
        para.render(group_rect, &mut temp_buf);

        y += group_height;
        if i < groups.len() - 1 {
            y += 1; // gap between groups
        }
    }

    // Copy visible portion from temp buffer into the frame
    let scroll = app.messages_scroll;
    let frame_buf = f.buffer_mut();
    for dy in 0..inner.height {
        let src_y = scroll + dy;
        if src_y >= total_height {
            break;
        }
        for dx in 0..inner.width {
            if let Some(src_cell) = temp_buf.cell(Position { x: dx, y: src_y }) {
                if let Some(dst_cell) =
                    frame_buf.cell_mut(Position {
                        x: inner.x + dx,
                        y: inner.y + dy,
                    })
                {
                    *dst_cell = src_cell.clone();
                }
            }
        }
    }
}
