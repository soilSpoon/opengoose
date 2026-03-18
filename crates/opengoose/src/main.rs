// OpenGoose v0.2 — CLI 진입점
//
// 대화형 REPL + 헤드리스 `run` 모드.
// Board + Rig + Goose Agent를 와이어링하는 유일한 장소.
// 모든 것이 Board를 통과한다.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use goose::agents::extension::ExtensionConfig;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use goose::model::ModelConfig;
use goose::session::session_manager::SessionType;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
use opengoose_board::Board;
use opengoose_rig::mcp_tools::BoardClient;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[derive(Parser)]
#[command(name = "opengoose", version = "0.2.0")]
#[command(about = "Goose-native pull architecture with Wasteland-level agent autonomy")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 단일 작업 실행 후 종료
    Run {
        /// 실행할 작업 내용
        task: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opengoose=info,opengoose_rig=info,goose=error".into()),
        )
        .init();

    let cli = Cli::parse();

    let board = Arc::new(Mutex::new(Board::new()));
    let rig_id = RigId::new("main");
    let (agent, session_id) = create_agent(&board, &rig_id).await?;

    match cli.command {
        Some(Commands::Run { task }) => {
            run_headless(board, rig_id, agent, &session_id, &task).await?;
        }
        None => {
            run_repl(board, rig_id, agent, &session_id).await?;
        }
    }

    Ok(())
}

/// Goose Agent 생성 + Provider 설정 + Board 도구 주입.
async fn create_agent(board: &Arc<Mutex<Board>>, rig_id: &RigId) -> Result<(Agent, String)> {
    let provider_name =
        std::env::var("GOOSE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());

    let agent = Agent::new();

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let session = agent
        .config
        .session_manager
        .create_session(cwd, "opengoose".into(), SessionType::User)
        .await
        .context("failed to create session")?;

    let provider = match std::env::var("GOOSE_MODEL") {
        Ok(model_name) => {
            info!(provider = %provider_name, model = %model_name, "creating agent");
            let model_config = ModelConfig::new(&model_name)
                .context("invalid model config")?
                .with_canonical_limits(&provider_name);
            goose::providers::create(&provider_name, model_config, vec![]).await
        }
        Err(_) => {
            info!(provider = %provider_name, model = "default", "creating agent");
            goose::providers::create_with_default_model(&provider_name, vec![]).await
        }
    }
    .context("failed to create provider")?;

    agent
        .update_provider(provider, &session.id)
        .await
        .context("failed to set provider")?;

    // Board 도구 주입
    let board_client = Arc::new(BoardClient::new(Arc::clone(board), rig_id.clone()));
    let board_config = ExtensionConfig::Platform {
        name: "board".into(),
        description: "Wanted Board — work items, claim, submit".into(),
        display_name: Some("Board".into()),
        bundled: None,
        available_tools: vec![],
    };
    agent
        .extension_manager
        .add_client("board".into(), board_config, board_client, None, None)
        .await;

    agent
        .extend_system_prompt(
            "opengoose".to_string(),
            "You are an OpenGoose rig. You have access to a Wanted Board via board__ tools. \
             Use board__read_board to see available work, board__claim_next to claim work, \
             and board__submit to complete work. You can also create new tasks with board__create_task."
                .to_string(),
        )
        .await;

    Ok((agent, session.id))
}

// ── 대화형 REPL ─────────────────────────────────────────────

async fn run_repl(
    board: Arc<Mutex<Board>>,
    rig_id: RigId,
    agent: Agent,
    session_id: &str,
) -> Result<()> {
    println!("OpenGoose v0.2 (Ctrl+D to exit)");
    println!("Commands: /board, /task \"...\"\n");

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // 명령 파싱
        if input == "/board" {
            show_board(&board).await;
            continue;
        }

        if let Some(task_title) = input.strip_prefix("/task ") {
            let task_title = task_title.trim().trim_matches('"');
            if task_title.is_empty() {
                println!("Usage: /task \"task description\"");
                continue;
            }
            handle_task(&board, &rig_id, &agent, session_id, task_title).await;
            continue;
        }

        // 일반 대화: Board에 게시 → Agent 실행 → 스트리밍 출력
        {
            let mut b = board.lock().await;
            b.post(PostWorkItem {
                title: input.to_string(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: Priority::P1,
            });
        }

        run_agent_streaming(&agent, session_id, input).await;
        println!();
    }

    println!("\nBye!");
    Ok(())
}

// ── /board 명령 ──────────────────────────────────────────────

async fn show_board(board: &Arc<Mutex<Board>>) {
    let board = board.lock().await;
    let items = board.list();

    let open: Vec<_> = items
        .iter()
        .filter(|i| i.status == opengoose_board::Status::Open)
        .collect();
    let claimed: Vec<_> = items
        .iter()
        .filter(|i| i.status == opengoose_board::Status::Claimed)
        .collect();
    let done: Vec<_> = items
        .iter()
        .filter(|i| i.status == opengoose_board::Status::Done)
        .collect();

    println!();
    println!("  Board: {} open · {} claimed · {} done", open.len(), claimed.len(), done.len());

    if !open.is_empty() {
        println!("  Open:");
        for item in &open {
            println!("    ○ #{} {:?} \"{}\"", item.id, item.priority, item.title);
        }
    }

    if !claimed.is_empty() {
        println!("  Claimed:");
        for item in &claimed {
            let by = item.claimed_by.as_ref().map(|r| r.0.as_str()).unwrap_or("?");
            println!("    ● #{} \"{}\" (by {})", item.id, item.title, by);
        }
    }

    if !done.is_empty() {
        println!("  Done (recent):");
        for item in done.iter().rev().take(5) {
            println!("    ✓ #{} \"{}\"", item.id, item.title);
        }
    }

    println!();
}

// ── /task 명령 ───────────────────────────────────────────────

async fn handle_task(
    board: &Arc<Mutex<Board>>,
    rig_id: &RigId,
    agent: &Agent,
    session_id: &str,
    title: &str,
) {
    // Board에 게시
    let item = {
        let mut b = board.lock().await;
        b.post(PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: rig_id.clone(),
            priority: Priority::P1,
        })
    };

    println!("● #{} \"{}\" — posted to board", item.id, item.title);

    // Rig가 Board에서 claim
    {
        let mut b = board.lock().await;
        match b.claim(item.id, rig_id) {
            Ok(_) => println!("  → claimed by {}", rig_id),
            Err(e) => {
                eprintln!("  → claim failed: {e}");
                return;
            }
        }
    }

    // Agent 실행
    let prompt = format!(
        "You have claimed work item #{}: \"{}\". Please complete this task.",
        item.id, item.title
    );
    run_agent_streaming(agent, session_id, &prompt).await;

    // 완료 제출
    {
        let mut b = board.lock().await;
        match b.submit(item.id, rig_id) {
            Ok(_) => println!("\n✓ #{} completed", item.id),
            Err(e) => eprintln!("\n⚠ submit failed: {e}"),
        }
    }
}

// ── Agent 스트리밍 실행 ──────────────────────────────────────

async fn run_agent_streaming(agent: &Agent, session_id: &str, input: &str) {
    let message = Message::user().with_text(input);
    let session_config = SessionConfig {
        id: session_id.to_string(),
        schedule_id: None,
        max_turns: None,
        retry_config: None,
    };

    match agent.reply(message, session_config, None).await {
        Ok(stream) => {
            tokio::pin!(stream);
            while let Some(event) = stream.next().await {
                if let Ok(AgentEvent::Message(msg)) = event {
                    print_message(&msg);
                }
            }
            println!();
        }
        Err(e) => {
            eprintln!("Error: {e}");
        }
    }
}

// ── 헤드리스 모드 ────────────────────────────────────────────

async fn run_headless(
    board: Arc<Mutex<Board>>,
    rig_id: RigId,
    agent: Agent,
    session_id: &str,
    task: &str,
) -> Result<()> {
    {
        let mut b = board.lock().await;
        b.post(PostWorkItem {
            title: task.to_string(),
            description: String::new(),
            created_by: rig_id.clone(),
            priority: Priority::P1,
        });
    }

    run_agent_streaming(&agent, session_id, task).await;
    Ok(())
}

// ── 출력 헬퍼 ────────────────────────────────────────────────

fn print_message(msg: &Message) {
    use goose::conversation::message::MessageContent;
    if msg.role == rmcp::model::Role::Assistant {
        for content in &msg.content {
            if let MessageContent::Text(text) = content {
                print!("{}", text.text);
                io::stdout().flush().ok();
            }
        }
    }
}
