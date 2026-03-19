use opengoose_board::work_item::Status;
use unicode_width::UnicodeWidthChar;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::app::{App, ChatLine};

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // 3단 분할: 상단(Board+Rigs), 중앙(Chat), 하단(Input)
    let chunks = Layout::vertical([
        Constraint::Length(top_panel_height(app)),
        Constraint::Min(6),
        Constraint::Length(3),
    ])
    .split(area);

    render_top(frame, app, chunks[0]);
    render_chat(frame, app, chunks[1]);
    render_input(frame, app, chunks[2]);
}

fn top_panel_height(app: &App) -> u16 {
    let active = app.active_items().len() + app.recent_done().len();
    let rigs = app.rigs.len();
    let rows = active.max(rigs).max(1) as u16;
    rows + 3 // border + header + 1 padding
}

// ── 상단: Board (좌) + Rigs (우) ────────────────────────────

fn render_top(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::horizontal([
        Constraint::Percentage(65),
        Constraint::Percentage(35),
    ])
    .split(area);

    render_board(frame, app, chunks[0]);
    render_rigs(frame, app, chunks[1]);
}

fn render_board(frame: &mut Frame, app: &App, area: Rect) {
    let (open, claimed, done) = app.board_summary();
    let title = format!(" Board — {open} open · {claimed} claimed · {done} done ");

    let mut items: Vec<ListItem> = Vec::new();

    // Active items (Open + Claimed)
    for item in app.active_items() {
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

        let line = Line::from(vec![
            Span::styled(format!("{icon} "), style),
            Span::styled(format!("#{}", item.id), Style::default().fg(Color::Cyan)),
            Span::raw(format!(" {:?} ", item.priority)),
            Span::styled(format!("\"{}\"", item.title), style),
            Span::styled(claimed_by, Style::default().fg(Color::DarkGray)),
        ]);
        items.push(ListItem::new(line));
    }

    // Recent done (dimmed)
    for item in app.recent_done() {
        let line = Line::from(vec![
            Span::styled("✓ ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("#{} \"{}\"", item.id, item.title),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        items.push(ListItem::new(line));
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(list, area);
}

fn render_rigs(frame: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();

    for rig in &app.rigs {
        let status_icon = rig.status.icon();
        let trust_style = match rig.trust_level.as_str() {
            "L3" => Style::default().fg(Color::Green),
            "L2.5" | "L2" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::DarkGray),
        };

        let line = Line::from(vec![
            Span::styled(
                format!("{:<12}", rig.id),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{:<4}", rig.trust_level), trust_style),
            Span::raw(format!(" {status_icon}")),
        ]);
        items.push(ListItem::new(line));
    }

    if items.is_empty() {
        items.push(ListItem::new(
            Line::from(Span::styled("(no rigs)", Style::default().fg(Color::DarkGray))),
        ));
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Rigs ")
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(list, area);
}

// ── 중앙: Chat ──────────────────────────────────────────────

fn render_chat(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize; // borders

    let lines: Vec<Line> = app
        .chat_lines
        .iter()
        .flat_map(|cl| chat_line_to_lines(cl))
        .collect();

    // 자동 스크롤: 맨 아래로
    let total = lines.len();
    let skip = if app.scroll_offset == 0 {
        total.saturating_sub(inner_height)
    } else {
        total.saturating_sub(inner_height + app.scroll_offset as usize)
    };

    let visible: Vec<Line> = lines.into_iter().skip(skip).collect();

    let busy_indicator = if app.agent_busy { " ⏳" } else { "" };
    let title = format!(" Chat{busy_indicator} ");

    let paragraph = Paragraph::new(visible)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn chat_line_to_lines(cl: &ChatLine) -> Vec<Line<'_>> {
    match cl {
        ChatLine::User(text) => vec![Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(text.as_str(), Style::default().fg(Color::Cyan)),
        ])],
        ChatLine::Agent(text) => {
            // 긴 응답은 여러 줄로 분할
            text.lines()
                .map(|line| {
                    Line::from(Span::styled(line, Style::default().fg(Color::White)))
                })
                .collect()
        }
        ChatLine::System(text) => vec![Line::from(Span::styled(
            text.as_str(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::ITALIC),
        ))],
    }
}

// ── 하단: Input ─────────────────────────────────────────────

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let paragraph = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Cyan)),
        Span::raw(&app.input),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(paragraph, area);

    // 커서 위치: 커서 앞 문자들의 display width 합산 (unicode-width 사용)
    let display_width: u16 = app
        .input
        .chars()
        .take(app.cursor_pos)
        .map(|c| c.width().unwrap_or(0) as u16)
        .sum();
    frame.set_cursor_position((area.x + 3 + display_width, area.y + 1));
}
