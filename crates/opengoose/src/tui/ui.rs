use opengoose_board::work_item::Status;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthChar;

use super::app::{App, ChatLine, Tab};
use super::log_entry::LogEntry;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if app.tab_bar_visible {
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);

        render_tab_bar(frame, app, chunks[0]);
        render_current_tab(frame, app, chunks[1]);
    } else {
        render_current_tab(frame, app, area);
    }
}

fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let tabs: Vec<Span> = Tab::ALL
        .iter()
        .enumerate()
        .flat_map(|(i, tab)| {
            let style = if *tab == app.current_tab {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let mut spans = vec![Span::styled(format!(" {} ", tab.label()), style)];
            if i < Tab::ALL.len() - 1 {
                spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
            }
            spans
        })
        .collect();

    frame.render_widget(Paragraph::new(Line::from(tabs)), area);
}

fn render_current_tab(frame: &mut Frame, app: &App, area: Rect) {
    match app.current_tab {
        Tab::Chat => {
            let chunks = Layout::vertical([Constraint::Min(6), Constraint::Length(3)]).split(area);
            render_chat(frame, app, chunks[0]);
            render_input(frame, app, chunks[1]);
        }
        Tab::Board => {
            let chunks =
                Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(area);
            render_board(frame, app, chunks[0]);
            render_rigs(frame, app, chunks[1]);
        }
        Tab::Logs => {
            render_logs(frame, app, area);
        }
    }
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
            Span::styled(format!("{:<12}", rig.id), Style::default().fg(Color::White)),
            Span::styled(format!("{:<4}", rig.trust_level), trust_style),
            Span::raw(format!(" {status_icon}")),
        ]);
        items.push(ListItem::new(line));
    }

    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "(no rigs)",
            Style::default().fg(Color::DarkGray),
        ))));
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

    let lines: Vec<Line> = app.chat_lines.iter().flat_map(chat_line_to_lines).collect();

    // 자동 스크롤: 맨 아래로
    let total = lines.len();
    let skip = if app.scroll_offset == 0 {
        total.saturating_sub(inner_height)
    } else {
        total.saturating_sub(inner_height + app.scroll_offset)
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
            Span::styled(
                "> ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(text.as_str(), Style::default().fg(Color::Cyan)),
        ])],
        ChatLine::Agent(text) => {
            // 긴 응답은 여러 줄로 분할
            text.lines()
                .map(|line| Line::from(Span::styled(line, Style::default().fg(Color::White))))
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

// ── Logs ────────────────────────────────────────────────────

fn render_logs(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let visible = app.visible_logs();
    let total = visible.len();

    let skip = if app.log_scroll_offset == 0 {
        total.saturating_sub(inner_height)
    } else {
        total.saturating_sub(inner_height + app.log_scroll_offset)
    };

    let lines: Vec<Line> = visible
        .into_iter()
        .skip(skip)
        .take(inner_height)
        .map(|entry| format_log_entry(entry, app.log_verbose))
        .collect();

    let mode_label = if app.log_verbose {
        "verbose"
    } else {
        "structured"
    };
    let title = format!(" Logs ({mode_label}) — press v to toggle ");

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(paragraph, area);
}

fn format_log_entry(entry: &LogEntry, verbose: bool) -> Line<'static> {
    let time = entry.timestamp.format("%H:%M:%S").to_string();

    if verbose {
        let level_style = match entry.level {
            tracing::Level::ERROR => Style::default().fg(Color::Red),
            tracing::Level::WARN => Style::default().fg(Color::Yellow),
            tracing::Level::INFO => Style::default().fg(Color::Green),
            _ => Style::default().fg(Color::DarkGray),
        };

        Line::from(vec![
            Span::styled(time, Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(format!("{:<5}", entry.level), level_style),
            Span::raw(" "),
            Span::styled(entry.target.clone(), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::raw(entry.message.clone()),
        ])
    } else {
        let source = if entry.target.contains("::rig") {
            "worker"
        } else if entry.target.contains("evolver") {
            "evolver"
        } else {
            "system"
        };

        Line::from(vec![
            Span::styled(time, Style::default().fg(Color::DarkGray)),
            Span::styled(format!(" [{source}] "), Style::default().fg(Color::Cyan)),
            Span::raw(entry.message.clone()),
        ])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_line_to_lines_preserves_line_counts() {
        let user_line = ChatLine::User("hello".into());
        let user = chat_line_to_lines(&user_line);
        assert_eq!(user.len(), 1);

        let agent_line = ChatLine::Agent("a\nb\nc".into());
        let agent = chat_line_to_lines(&agent_line);
        assert_eq!(agent.len(), 3);

        let system_line = ChatLine::System("note".into());
        let system = chat_line_to_lines(&system_line);
        assert_eq!(system.len(), 1);
    }
}
