use opengoose_board::work_item::Status;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use super::super::app::App;

pub fn render_board(frame: &mut Frame, app: &App, area: Rect) {
    let (open, claimed, done) = app.board_summary();
    let title = format!(" Board — {open} open · {claimed} claimed · {done} done ");

    let active_items: Vec<ListItem> = app
        .active_items()
        .iter()
        .map(|item| {
            let (icon, style) = match item.status {
                Status::Open => ("○", Style::default().fg(Color::White)),
                Status::Claimed => ("●", Style::default().fg(Color::Yellow)),
                _ => ("·", Style::default()),
            };

            let claimed_by = if let Some(ref rig) = item.claimed_by {
                format!(" ({})", rig.0)
            } else {
                String::new()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{icon} "), style),
                Span::styled(format!("#{}", item.id), Style::default().fg(Color::Cyan)),
                Span::raw(format!(" {:?} ", item.priority)),
                Span::styled(format!("\"{}\"", item.title), style),
                Span::styled(claimed_by, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let done_items: Vec<ListItem> = app
        .recent_done()
        .iter()
        .map(|item| {
            ListItem::new(Line::from(vec![
                Span::styled("✓ ", Style::default().fg(Color::Green)),
                Span::styled(
                    format!("#{} \"{}\"", item.id, item.title),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let items: Vec<ListItem> = active_items.into_iter().chain(done_items).collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(list, area);
}

pub fn render_rigs(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = if app.board.rigs.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "(no rigs)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.board
            .rigs
            .iter()
            .map(|rig| {
                let status_icon = rig.status.icon();
                let trust_style = match rig.trust_level.as_str() {
                    "L3" => Style::default().fg(Color::Green),
                    "L2.5" | "L2" => Style::default().fg(Color::Yellow),
                    _ => Style::default().fg(Color::DarkGray),
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<12}", rig.id), Style::default().fg(Color::White)),
                    Span::styled(format!("{:<4}", rig.trust_level), trust_style),
                    Span::raw(format!(" {status_icon}")),
                ]))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Rigs ")
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(list, area);
}
