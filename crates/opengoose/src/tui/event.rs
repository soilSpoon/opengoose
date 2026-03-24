use anyhow::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use goose::agents::AgentEvent;
use goose::conversation::message::MessageContent;
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
use opengoose_rig::rig::Operator;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use super::app::{App, ChatLine, RigInfo, RigStatus, Tab};
use super::log_entry::LogEntry;
use super::ui;

/// Agent → TUI 이벤트
pub enum AgentMsg {
    /// 스트리밍 텍스트 조각
    Text(String),
    /// 응답 완료
    Done,
}

pub async fn run_tui(
    board: Arc<Board>,
    operator: Arc<Operator>,
    mut log_rx: tokio::sync::mpsc::Receiver<LogEntry>,
) -> Result<()> {
    // 터미널 설정
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // 초기 Board 로딩
    if let Ok(items) = board.list().await {
        app.board.items = items;
    }
    load_rigs(&board, &mut app).await;

    // Agent 통신 채널
    let (agent_tx, mut agent_rx) = mpsc::channel::<AgentMsg>(100);

    // crossterm 이벤트 스트림
    let mut reader = EventStream::new();

    // Board 갱신 타이머 (2초)
    let mut board_tick = interval(Duration::from_secs(2));

    // 렌더링 타이머 (60fps → 16ms)
    let mut render_tick = interval(Duration::from_millis(50));

    // 초기 렌더링
    terminal.draw(|f| ui::render(f, &app))?;

    loop {
        tokio::select! {
            // 키보드 입력
            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if handle_key(key, &mut app, &agent_tx, &board, &operator).await {
                            break;
                        }
                    }
                    Some(Ok(Event::Resize(_, _))) => {
                        // 리사이즈 시 즉시 다시 그리기
                    }
                    Some(Err(_)) | None => break,
                    _ => {}
                }
            }
            // Agent 응답
            Some(msg) = agent_rx.recv() => {
                match msg {
                    AgentMsg::Text(text) => {
                        app.append_agent_text(&text);
                    }
                    AgentMsg::Done => {
                        app.agent_busy = false;
                        // 완료 후 Board 갱신
                        if let Ok(items) = board.list().await {
                            app.board.items = items;
                        }
                    }
                }
            }
            // 로그 수신
            Some(entry) = log_rx.recv() => {
                app.push_log(entry);
            }
            // Board 주기적 갱신
            _ = board_tick.tick() => {
                if let Ok(items) = board.list().await {
                    app.board.items = items;
                }
                load_rigs(&board, &mut app).await;
            }
            // 렌더링
            _ = render_tick.tick() => {
                terminal.draw(|f| ui::render(f, &app))?;
            }
        }
    }

    // 터미널 복원
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

/// 키 이벤트 처리. quit이면 true 반환.
async fn handle_key(
    key: KeyEvent,
    app: &mut App,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) -> bool {
    match (key.code, key.modifiers) {
        // ── 전역 단축키 ──
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return true;
        }
        (KeyCode::Char('1'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Chat;
        }
        (KeyCode::Char('2'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Board;
        }
        (KeyCode::Char('3'), KeyModifiers::CONTROL) => {
            app.current_tab = Tab::Logs;
        }
        (KeyCode::Tab, KeyModifiers::NONE) => {
            app.current_tab = app.current_tab.next();
        }
        (KeyCode::BackTab, _) => {
            app.current_tab = app.current_tab.prev();
        }
        (KeyCode::Char('\\'), KeyModifiers::CONTROL) => {
            app.tab_bar_visible = !app.tab_bar_visible;
        }

        // ── 스크롤 (탭별) ──
        (KeyCode::Up, KeyModifiers::NONE) => match app.current_tab {
            Tab::Chat if app.chat.input.is_empty() => {
                app.chat.scroll_offset = app.chat.scroll_offset.saturating_add(1);
            }
            Tab::Logs => {
                app.logs.scroll_offset = app.logs.scroll_offset.saturating_add(1);
                app.logs.auto_scroll = false;
            }
            _ => {}
        },
        (KeyCode::Down, KeyModifiers::NONE) => match app.current_tab {
            Tab::Chat if app.chat.input.is_empty() => {
                app.chat.scroll_offset = app.chat.scroll_offset.saturating_sub(1);
            }
            Tab::Logs => {
                app.logs.scroll_offset = app.logs.scroll_offset.saturating_sub(1);
                if app.logs.scroll_offset == 0 {
                    app.logs.auto_scroll = true;
                }
            }
            _ => {}
        },
        (KeyCode::PageUp, _) => match app.current_tab {
            Tab::Chat => {
                app.chat.scroll_offset = app.chat.scroll_offset.saturating_add(10);
            }
            Tab::Logs => {
                app.logs.scroll_offset = app.logs.scroll_offset.saturating_add(10);
                app.logs.auto_scroll = false;
            }
            _ => {}
        },
        (KeyCode::PageDown, _) => match app.current_tab {
            Tab::Chat => {
                app.chat.scroll_offset = app.chat.scroll_offset.saturating_sub(10);
            }
            Tab::Logs => {
                app.logs.scroll_offset = app.logs.scroll_offset.saturating_sub(10);
                if app.logs.scroll_offset == 0 {
                    app.logs.auto_scroll = true;
                }
            }
            _ => {}
        },

        // ── 탭별 키 처리 ──
        _ => match app.current_tab {
            Tab::Chat => handle_chat_key(key, app, agent_tx, board, operator).await,
            Tab::Board => {}
            Tab::Logs => handle_logs_key(key, app),
        },
    }

    false
}

/// Chat 탭 전용 키 처리
async fn handle_chat_key(
    key: KeyEvent,
    app: &mut App,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) {
    match key.code {
        KeyCode::Enter => {
            if let Some(text) = app.submit_input() {
                handle_input(app, &text, agent_tx, board, operator).await;
            }
        }
        KeyCode::Char(c)
            if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
        {
            let byte_pos = app.cursor_byte_pos();
            app.chat.input.insert(byte_pos, c);
            app.chat.cursor_pos += 1;
        }
        KeyCode::Backspace => {
            if app.chat.cursor_pos > 0 {
                app.chat.cursor_pos -= 1;
                let byte_pos = app.cursor_byte_pos();
                let ch = app.chat.input[byte_pos..].chars().next().expect("cursor_pos > 0 guarantees non-empty slice");
                app.chat
                    .input
                    .replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        KeyCode::Delete => {
            if app.chat.cursor_pos < app.char_count() {
                let byte_pos = app.cursor_byte_pos();
                let ch = app.chat.input[byte_pos..].chars().next().expect("cursor_pos < char_count guarantees non-empty slice");
                app.chat
                    .input
                    .replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        KeyCode::Left => app.chat.cursor_pos = app.chat.cursor_pos.saturating_sub(1),
        KeyCode::Right => {
            if app.chat.cursor_pos < app.char_count() {
                app.chat.cursor_pos += 1;
            }
        }
        KeyCode::Home => app.chat.cursor_pos = 0,
        KeyCode::End => app.chat.cursor_pos = app.char_count(),
        _ => {}
    }
}

/// Logs 탭 전용 키 처리
fn handle_logs_key(key: KeyEvent, app: &mut App) {
    if let KeyCode::Char('v') = key.code {
        app.logs.verbose = !app.logs.verbose;
        app.logs.scroll_offset = 0;
    }
}

/// 사용자 입력 처리 (대화 또는 명령)
async fn handle_input(
    app: &mut App,
    text: &str,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    operator: &Arc<Operator>,
) {
    // /board 명령
    if text == "/board" {
        if let Ok(items) = board.list().await {
            app.board.items = items.clone();
            let (open, claimed, done) = app.board_summary();
            app.push_chat(ChatLine::System(format!(
                "Board: {open} open · {claimed} claimed · {done} done"
            )));
        }
        return;
    }

    // /task 명령
    if text == "/task" {
        app.push_chat(ChatLine::System("Usage: /task \"description\"".into()));
        return;
    }
    if let Some(task_title) = text.strip_prefix("/task ") {
        let task_title = task_title.trim().trim_matches('"');
        if task_title.is_empty() {
            app.push_chat(ChatLine::System("Usage: /task \"description\"".into()));
            return;
        }
        handle_task(app, task_title, board).await;
        return;
    }

    // /quit
    if text == "/quit" || text == "/q" {
        app.should_quit = true;
        return;
    }

    // 일반 대화 → Operator로 전송
    if app.agent_busy {
        app.push_chat(ChatLine::System("Agent is busy...".into()));
        return;
    }

    app.agent_busy = true;
    spawn_operator_reply(operator.clone(), text.to_string(), agent_tx.clone());
}

/// /task 처리: Board에 post → Worker가 자동으로 pick up
async fn handle_task(app: &mut App, title: &str, board: &Arc<Board>) {
    match board
        .post(PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: RigId::new("operator"),
            priority: Priority::P1,
            tags: vec![],
        })
        .await
    {
        Ok(item) => {
            app.push_chat(ChatLine::System(format!(
                "● #{} \"{}\" — posted (Worker will pick it up)",
                item.id, item.title
            )));
            if let Ok(items) = board.list().await {
                app.board.items = items;
            }
        }
        Err(e) => {
            app.push_chat(ChatLine::System(format!("Post failed: {e}")));
        }
    }
}

/// Operator.chat_streaming()을 별도 tokio task로 실행, 스트리밍으로 전송
fn spawn_operator_reply(operator: Arc<Operator>, input: String, tx: mpsc::Sender<AgentMsg>) {
    tokio::spawn(async move {
        match operator.chat_streaming(&input).await {
            Ok(stream) => {
                tokio::pin!(stream);
                while let Some(event) = stream.next().await {
                    match event {
                        Ok(AgentEvent::Message(msg))
                            if msg.role == rmcp::model::Role::Assistant =>
                        {
                            for content in &msg.content {
                                if let MessageContent::Text(text) = content {
                                    let _ = tx.send(AgentMsg::Text(text.text.clone())).await;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx
                                .send(AgentMsg::Text(format!("\n⚠ Stream error: {e}")))
                                .await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(AgentMsg::Text(format!("Error: {e}"))).await;
            }
        }
        let _ = tx.send(AgentMsg::Done).await;
    });
}

/// DB에서 Rig 정보 로딩
async fn load_rigs(board: &Board, app: &mut App) {
    if let Ok(rigs) = board.list_rigs().await {
        let mut infos = Vec::new();
        for rig in &rigs {
            let trust = board.trust_level(&rig.id).await.unwrap_or("L1");
            let is_working = app.board.items.iter().any(|i| {
                i.status == Status::Claimed && i.claimed_by.as_ref().is_some_and(|r| r.0 == rig.id)
            });

            infos.push(RigInfo {
                id: rig.id.clone(),
                trust_level: trust.to_string(),
                status: if is_working {
                    RigStatus::Working
                } else {
                    RigStatus::Idle
                },
            });
        }
        app.board.rigs = infos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{ChatLine, RigStatus};
    use opengoose_board::work_item::RigId;

    fn make_operator(session_id: &str) -> std::sync::Arc<Operator> {
        std::sync::Arc::new(Operator::without_board(
            RigId::new("test"),
            goose::agents::Agent::new(),
            session_id,
        ))
    }

    #[tokio::test]
    async fn handle_key_char_and_backspace() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s1");
        let (tx, mut _rx) = mpsc::channel(4);

        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!should_quit);
        assert_eq!(app.chat.input, "a");
        assert_eq!(app.chat.cursor_pos, 1);

        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!should_quit);
        assert_eq!(app.chat.input, "");
        assert_eq!(app.chat.cursor_pos, 0);
    }

    #[tokio::test]
    async fn handle_key_escape_and_scrolling() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s1");
        let (tx, mut _rx) = mpsc::channel(4);

        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(should_quit);
        assert!(app.should_quit);
        assert_eq!(app.chat.scroll_offset, 0);
    }

    #[tokio::test]
    async fn handle_key_scroll_keys_when_input_empty() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s1");
        let (tx, mut _rx) = mpsc::channel(4);

        app.chat.scroll_offset = 0;
        handle_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 1);

        app.chat.scroll_offset = 3;
        handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 2);
    }

    #[tokio::test]
    async fn handle_key_board_command_refreshes_items_and_pushes_system_line() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "Open item".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("creator"),
                priority: opengoose_board::work_item::Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        let operator = make_operator("s1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/board".into();
        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(!should_quit);
        assert_eq!(app.board.items.len(), 1);
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::System(text) if text.starts_with("Board:")))
        );
    }

    #[tokio::test]
    async fn handle_key_task_command_posts_item_without_agent_spawn() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s2");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/task \"implement feature\"".into();
        app.agent_busy = true;
        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(!should_quit);
        assert!(!app.should_quit);
        assert_eq!(board.list().await.unwrap().len(), 1);
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::System(text) if text.contains("posted")))
        );
    }

    #[tokio::test]
    async fn handle_key_invalid_task_usage() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s3");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/task".into();
        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(!should_quit);
        assert!(app.chat.lines.iter().any(
            |line| matches!(line, ChatLine::System(text) if text == "Usage: /task \"description\"")
        ));
    }

    #[tokio::test]
    async fn handle_key_busy_chat_does_not_send_to_agent() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("s4");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "hello".into();
        app.agent_busy = true;
        let should_quit = handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(!should_quit);
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::System(text) if text == "Agent is busy..."))
        );
    }

    #[tokio::test]
    async fn load_rigs_marks_working_status_from_board_snapshot() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        board
            .register_rig("r1", "ai", Some("worker"), Some(&["tag".into()]))
            .await
            .unwrap();
        let item = board
            .post(opengoose_board::work_item::PostWorkItem {
                title: "Active".into(),
                description: String::new(),
                created_by: opengoose_board::work_item::RigId::new("creator"),
                priority: opengoose_board::work_item::Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .claim(item.id, &opengoose_board::work_item::RigId::new("r1"))
            .await
            .unwrap();
        app.board.items = board.list().await.unwrap();

        load_rigs(&board, &mut app).await;

        // Board always includes "human" and "evolver" system rigs, plus "r1" = 3 total.
        let r1 = app
            .board
            .rigs
            .iter()
            .find(|r| r.id == "r1")
            .expect("r1 not found");
        assert_eq!(r1.status.icon(), "⚙");
    }

    #[test]
    fn rig_status_icons_used_in_ui() {
        assert_eq!(RigStatus::Idle.icon(), "💤");
        assert_eq!(RigStatus::Working.icon(), "⚙");
    }

    #[tokio::test]
    async fn handle_key_tab_cycles_forward() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("tab1");
        let (tx, _rx) = mpsc::channel(4);

        assert_eq!(app.current_tab, Tab::Chat);
        handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Board);
        handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Logs);
        handle_key(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Chat);
    }

    #[tokio::test]
    async fn handle_key_backtab_cycles_backward() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("tab2");
        let (tx, _rx) = mpsc::channel(4);

        handle_key(
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Logs);
        handle_key(
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Board);
    }

    #[tokio::test]
    async fn handle_key_ctrl_1_2_3_jump_to_tabs() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("tab3");
        let (tx, _rx) = mpsc::channel(4);

        handle_key(
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Board);
        handle_key(
            KeyEvent::new(KeyCode::Char('3'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Logs);
        handle_key(
            KeyEvent::new(KeyCode::Char('1'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.current_tab, Tab::Chat);
    }

    #[tokio::test]
    async fn handle_key_ctrl_backslash_toggles_tab_bar() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("tabbar1");
        let (tx, _rx) = mpsc::channel(4);

        assert!(app.tab_bar_visible);
        handle_key(
            KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!app.tab_bar_visible);
        handle_key(
            KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app.tab_bar_visible);
    }

    #[tokio::test]
    async fn handle_key_v_toggles_log_verbose_in_logs_tab() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("verbose1");
        let (tx, _rx) = mpsc::channel(4);

        app.current_tab = Tab::Logs;
        app.logs.scroll_offset = 5;
        handle_key(
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app.logs.verbose);
        assert_eq!(app.logs.scroll_offset, 0);
        handle_key(
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!app.logs.verbose);
    }

    #[tokio::test]
    async fn handle_key_pageup_pagedown_in_chat_tab() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("page1");
        let (tx, _rx) = mpsc::channel(4);

        handle_key(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 10);
        handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 0);
        handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 0);
    }

    #[tokio::test]
    async fn handle_key_pageup_pagedown_in_logs_tab() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("page2");
        let (tx, _rx) = mpsc::channel(4);

        app.current_tab = Tab::Logs;
        app.logs.auto_scroll = true;
        handle_key(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.logs.scroll_offset, 10);
        assert!(!app.logs.auto_scroll);
        handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.logs.scroll_offset, 0);
        assert!(app.logs.auto_scroll);
    }

    #[tokio::test]
    async fn handle_key_pageup_pagedown_in_board_tab_are_noop() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("boardpage1");
        let (tx, _rx) = mpsc::channel(4);

        app.current_tab = Tab::Board;
        handle_key(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 0);
        assert_eq!(app.logs.scroll_offset, 0);
        handle_key(
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 0);
        assert_eq!(app.logs.scroll_offset, 0);
    }

    #[tokio::test]
    async fn handle_key_up_down_in_logs_tab() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("logscroll1");
        let (tx, _rx) = mpsc::channel(4);

        app.current_tab = Tab::Logs;
        app.logs.auto_scroll = true;
        handle_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.logs.scroll_offset, 1);
        assert!(!app.logs.auto_scroll);
        handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.logs.scroll_offset, 0);
        assert!(app.logs.auto_scroll);
    }

    #[tokio::test]
    async fn handle_key_up_down_in_chat_tab_with_nonempty_input_are_noop() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("chatscroll1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "typing".into();
        app.chat.cursor_pos = 6;
        app.chat.scroll_offset = 2;
        handle_key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 2);
        handle_key(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.scroll_offset, 2);
    }

    #[tokio::test]
    async fn handle_key_left_right_home_end_cursor_movement() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("cursor1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "hello".into();
        app.chat.cursor_pos = 5;

        handle_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 4);
        handle_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 5);
        handle_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 5); // at end, no-op
        handle_key(
            KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 0);
        handle_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 0); // saturating_sub
        handle_key(
            KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.cursor_pos, 5);
    }

    #[tokio::test]
    async fn handle_key_delete_removes_char_at_cursor() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("delete1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "abc".into();
        app.chat.cursor_pos = 1;
        handle_key(
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.input, "ac");
        app.chat.cursor_pos = 2;
        handle_key(
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.input, "ac"); // at end, no-op
    }

    #[tokio::test]
    async fn handle_key_quit_command_sets_should_quit() {
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("quit1");
        let (tx, _rx) = mpsc::channel(4);

        let mut app = App::new();
        app.chat.input = "/quit".into();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app.should_quit);

        let mut app2 = App::new();
        app2.chat.input = "/q".into();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app2,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app2.should_quit);
    }

    #[tokio::test]
    async fn handle_key_enter_with_empty_input_is_noop() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("enter_empty");
        let (tx, _rx) = mpsc::channel(4);

        let initial_lines = app.chat.lines.len();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert_eq!(app.chat.lines.len(), initial_lines);
        assert!(!app.agent_busy);
    }

    #[tokio::test]
    async fn handle_key_enter_when_not_busy_sets_agent_busy() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("notbusy1");
        let (tx, _rx) = mpsc::channel(16);

        app.chat.input = "hello there".into();
        app.agent_busy = false;
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(app.agent_busy);
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::User(text) if text == "hello there"))
        );
    }

    #[tokio::test]
    async fn handle_key_board_refreshes_on_board_command() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("boardcmd1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/board".into();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(
            app.chat
                .lines
                .iter()
                .any(|line| matches!(line, ChatLine::System(text) if text.contains("Board:")))
        );
    }

    #[tokio::test]
    async fn load_rigs_with_no_claimed_items_all_rigs_are_idle() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());

        app.board.items = board.list().await.unwrap();
        load_rigs(&board, &mut app).await;

        for rig in &app.board.rigs {
            assert!(
                matches!(rig.status, RigStatus::Idle),
                "expected idle for rig {}",
                rig.id
            );
        }
    }

    #[tokio::test]
    async fn load_rigs_registered_rig_without_claimed_item_is_idle() {
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());

        board
            .register_rig("worker42", "ai", Some("worker"), Some(&["tag".into()]))
            .await
            .unwrap();
        app.board.items = board.list().await.unwrap();
        load_rigs(&board, &mut app).await;

        let w = app
            .board
            .rigs
            .iter()
            .find(|r| r.id == "worker42")
            .expect("worker42 should appear");
        assert!(matches!(w.status, RigStatus::Idle));
    }

    #[tokio::test]
    async fn handle_key_unknown_key_hits_default_branch() {
        // Covers line 253: `_ => {}` default arm — pressing a key not handled
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("unknown1");
        let (tx, _rx) = mpsc::channel(4);

        let should_quit = handle_key(
            KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;
        assert!(!should_quit);
    }

    #[tokio::test]
    async fn handle_key_task_with_empty_title_shows_usage() {
        // Covers line 287: `return;` in `if task_title.is_empty()` branch
        // "/task \"\"" → strip_prefix("/task ") gives "\"\"", trim+trim_matches('"') → "" → empty
        // Note: submit_input() trims whitespace, so "/task " becomes "/task" (handled earlier).
        // We must use "/task \"\"" so it reaches strip_prefix and produces an empty title.
        let mut app = App::new();
        let board = std::sync::Arc::new(opengoose_board::Board::in_memory().await.unwrap());
        let operator = make_operator("empty-task1");
        let (tx, _rx) = mpsc::channel(4);

        app.chat.input = "/task \"\"".into();
        handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut app,
            &tx,
            &board,
            &operator,
        )
        .await;

        assert!(app.chat.lines.iter().any(|line| {
            matches!(line, ChatLine::System(t) if t == "Usage: /task \"description\"")
        }));
    }
}
