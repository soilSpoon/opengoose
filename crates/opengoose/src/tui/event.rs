use anyhow::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::{Message, MessageContent};
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use super::app::{App, ChatLine, RigInfo, RigStatus};
use super::ui;

/// Agent → TUI 이벤트
pub enum AgentMsg {
    /// 스트리밍 텍스트 조각
    Text(String),
    /// 응답 완료
    Done,
}

pub async fn run_tui(board: Arc<Board>, agent: Arc<Agent>, session_id: String) -> Result<()> {
    // 터미널 설정
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // 초기 Board 로딩
    if let Ok(items) = board.list().await {
        app.board_items = items;
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
                        if handle_key(key, &mut app, &agent_tx, &board, &agent, &session_id).await {
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
                            app.board_items = items;
                        }
                    }
                }
            }
            // Board 주기적 갱신
            _ = board_tick.tick() => {
                if let Ok(items) = board.list().await {
                    app.board_items = items;
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
    agent: &Arc<Agent>,
    session_id: &str,
) -> bool {
    match (key.code, key.modifiers) {
        // 종료
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return true;
        }
        // Enter — 입력 전송
        (KeyCode::Enter, _) => {
            if let Some(text) = app.submit_input() {
                handle_input(app, &text, agent_tx, board, agent, session_id).await;
            }
        }
        // 텍스트 입력
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            let byte_pos = app.cursor_byte_pos();
            app.input.insert(byte_pos, c);
            app.cursor_pos += 1;
        }
        // Backspace
        (KeyCode::Backspace, _) => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                let byte_pos = app.cursor_byte_pos();
                let ch = app.input[byte_pos..].chars().next().unwrap();
                app.input.replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        // Delete
        (KeyCode::Delete, _) => {
            if app.cursor_pos < app.char_count() {
                let byte_pos = app.cursor_byte_pos();
                let ch = app.input[byte_pos..].chars().next().unwrap();
                app.input.replace_range(byte_pos..byte_pos + ch.len_utf8(), "");
            }
        }
        // 커서 이동
        (KeyCode::Left, _) => {
            app.cursor_pos = app.cursor_pos.saturating_sub(1);
        }
        (KeyCode::Right, _) => {
            if app.cursor_pos < app.char_count() {
                app.cursor_pos += 1;
            }
        }
        (KeyCode::Home, _) => {
            app.cursor_pos = 0;
        }
        (KeyCode::End, _) => {
            app.cursor_pos = app.char_count();
        }
        // Chat 스크롤
        (KeyCode::Up, KeyModifiers::NONE) if app.input.is_empty() => {
            app.scroll_offset = app.scroll_offset.saturating_add(1);
        }
        (KeyCode::Down, KeyModifiers::NONE) if app.input.is_empty() => {
            app.scroll_offset = app.scroll_offset.saturating_sub(1);
        }
        (KeyCode::PageUp, _) => {
            app.scroll_offset = app.scroll_offset.saturating_add(10);
        }
        (KeyCode::PageDown, _) => {
            app.scroll_offset = app.scroll_offset.saturating_sub(10);
        }
        _ => {}
    }

    false
}

/// 사용자 입력 처리 (대화 또는 명령)
async fn handle_input(
    app: &mut App,
    text: &str,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    agent: &Arc<Agent>,
    session_id: &str,
) {
    // /board 명령
    if text == "/board" {
        if let Ok(items) = board.list().await {
            app.board_items = items.clone();
            let (open, claimed, done) = app.board_summary();
            app.push_chat(ChatLine::System(format!(
                "Board: {open} open · {claimed} claimed · {done} done"
            )));
        }
        return;
    }

    // /task 명령
    if let Some(task_title) = text.strip_prefix("/task ") {
        let task_title = task_title.trim().trim_matches('"');
        if task_title.is_empty() {
            app.push_chat(ChatLine::System("Usage: /task \"description\"".into()));
            return;
        }
        handle_task(app, task_title, agent_tx, board, agent, session_id).await;
        return;
    }

    // /quit
    if text == "/quit" || text == "/q" {
        app.should_quit = true;
        return;
    }

    // 일반 대화 → Agent로 전송
    if app.agent_busy {
        app.push_chat(ChatLine::System("Agent is busy...".into()));
        return;
    }

    app.agent_busy = true;
    spawn_agent_reply(agent.clone(), session_id.to_string(), text.to_string(), agent_tx.clone());
}

/// /task 처리: Board에 post → Agent에게 알림
async fn handle_task(
    app: &mut App,
    title: &str,
    agent_tx: &mpsc::Sender<AgentMsg>,
    board: &Arc<Board>,
    agent: &Arc<Agent>,
    session_id: &str,
) {
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
                "● #{} \"{}\" — posted",
                item.id, item.title
            )));

            // Board 즉시 갱신
            if let Ok(items) = board.list().await {
                app.board_items = items;
            }

            // Agent에게 알림
            if !app.agent_busy {
                app.agent_busy = true;
                let prompt = format!(
                    "New work item posted to the board:\n\
                     #{} \"{}\"\n\n\
                     Claim it with `opengoose board claim {}`, complete the task, \
                     then submit with `opengoose board submit {}`.",
                    item.id, item.title, item.id, item.id
                );
                spawn_agent_reply(agent.clone(), session_id.to_string(), prompt, agent_tx.clone());
            }
        }
        Err(e) => {
            app.push_chat(ChatLine::System(format!("Post failed: {e}")));
        }
    }
}

/// Agent.reply()를 별도 tokio task로 실행, 스트리밍으로 전송
fn spawn_agent_reply(
    agent: Arc<Agent>,
    session_id: String,
    input: String,
    tx: mpsc::Sender<AgentMsg>,
) {
    tokio::spawn(async move {
        let message = Message::user().with_text(&input);
        let session_config = SessionConfig {
            id: session_id,
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        match agent.reply(message, session_config, None).await {
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
                            let _ = tx.send(AgentMsg::Text(format!("\n⚠ Stream error: {e}"))).await;
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
            let is_working = app
                .board_items
                .iter()
                .any(|i| i.status == Status::Claimed && i.claimed_by.as_ref().is_some_and(|r| r.0 == rig.id));

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
        app.rigs = infos;
    }
}
