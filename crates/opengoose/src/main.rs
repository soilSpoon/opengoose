// OpenGoose v0.2 — CLI 진입점
//
// 대화형 REPL + 헤드리스 `run` + Board CLI.
// Board + Goose Agent를 와이어링. 모든 작업이 Board를 통과.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use goose::model::ModelConfig;
use goose::session::session_manager::SessionType;
use opengoose_board::db_board::DbBoard;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
use std::io::{self, Write};
use tracing::info;

fn db_url() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let dir = home.join(".opengoose");
    std::fs::create_dir_all(&dir).ok();
    format!("sqlite://{}?mode=rwc", dir.join("board.db").display())
}

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
    /// Board 관리
    Board {
        #[command(subcommand)]
        action: BoardAction,
    },
}

#[derive(Subcommand)]
enum BoardAction {
    /// 보드 상태 표시
    Status,
    /// claim 가능한 작업 목록
    Ready,
    /// 작업 claim
    Claim { id: i64 },
    /// 작업 완료
    Submit { id: i64 },
    /// 새 작업 게시
    Create {
        title: String,
        #[arg(long, default_value = "P1")]
        priority: String,
    },
    /// 작업 포기
    Abandon { id: i64 },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "opengoose=info,goose=error".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Board { action }) => {
            let board = DbBoard::connect(&db_url()).await?;
            run_board_command(&board, action).await
        }
        Some(Commands::Run { task }) => {
            let board = DbBoard::connect(&db_url()).await?;
            let (agent, session_id) = create_agent().await?;
            run_headless(&board, &agent, &session_id, &task).await
        }
        None => {
            let board = DbBoard::connect(&db_url()).await?;
            let (agent, session_id) = create_agent().await?;
            run_repl(&board, &agent, &session_id).await
        }
    }
}

// ── Board CLI ────────────────────────────────────────────────

async fn run_board_command(board: &DbBoard, action: BoardAction) -> Result<()> {
    let rig_id = RigId::new("cli");

    match action {
        BoardAction::Status => {
            show_board(board).await?;
        }
        BoardAction::Ready => {
            let items = board.ready().await?;
            if items.is_empty() {
                println!("No claimable items.");
            } else {
                for item in &items {
                    println!("#{} {:?} \"{}\"", item.id, item.priority, item.title);
                }
            }
        }
        BoardAction::Claim { id } => {
            let item = board.claim(id, &rig_id).await?;
            println!("Claimed #{}: \"{}\"", item.id, item.title);
        }
        BoardAction::Submit { id } => {
            let item = board.submit(id, &rig_id).await?;
            println!("Completed #{}: \"{}\"", item.id, item.title);
        }
        BoardAction::Create { title, priority } => {
            let priority = Priority::parse(&priority).unwrap_or_default();
            let item = board
                .post(PostWorkItem {
                    title,
                    description: String::new(),
                    created_by: rig_id,
                    priority,
                })
                .await?;
            println!("Created #{}: \"{}\" ({:?})", item.id, item.title, item.priority);
        }
        BoardAction::Abandon { id } => {
            let item = board.abandon(id).await?;
            println!("Abandoned #{}: \"{}\"", item.id, item.title);
        }
    }

    Ok(())
}

async fn show_board(board: &DbBoard) -> Result<()> {
    let items = board.list().await?;

    let open: Vec<_> = items.iter().filter(|i| i.status == Status::Open).collect();
    let claimed: Vec<_> = items.iter().filter(|i| i.status == Status::Claimed).collect();
    let done: Vec<_> = items.iter().filter(|i| i.status == Status::Done).collect();

    println!("Board: {} open · {} claimed · {} done", open.len(), claimed.len(), done.len());

    if !open.is_empty() {
        println!("\nOpen:");
        for item in &open {
            println!("  ○ #{} {:?} \"{}\"", item.id, item.priority, item.title);
        }
    }

    if !claimed.is_empty() {
        println!("\nClaimed:");
        for item in &claimed {
            let by = item.claimed_by.as_ref().map(|r| r.0.as_str()).unwrap_or("?");
            println!("  ● #{} \"{}\" (by {})", item.id, item.title, by);
        }
    }

    if !done.is_empty() {
        println!("\nDone (recent):");
        for item in done.iter().rev().take(5) {
            println!("  ✓ #{} \"{}\"", item.id, item.title);
        }
    }

    Ok(())
}

// ── Agent 생성 ───────────────────────────────────────────────

async fn create_agent() -> Result<(Agent, String)> {
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

    // Skills: Agent에게 Board CLI 사용법을 가르친다
    agent
        .extend_system_prompt(
            "opengoose".to_string(),
            "You are an OpenGoose rig. You have access to a Wanted Board via CLI commands.\n\
             Available commands (run via shell):\n\
             - opengoose board status    — show board state (open/claimed/done)\n\
             - opengoose board ready     — list claimable work items\n\
             - opengoose board claim ID  — claim a work item\n\
             - opengoose board submit ID — mark work item as done\n\
             - opengoose board create \"TITLE\" — post a new task\n\
             - opengoose board abandon ID — abandon a work item\n\
             \n\
             When given a task via /task, claim it from the board, complete the work, then submit."
                .to_string(),
        )
        .await;

    Ok((agent, session.id))
}

// ── 대화형 REPL ─────────────────────────────────────────────

async fn run_repl(board: &DbBoard, agent: &Agent, session_id: &str) -> Result<()> {
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

        if input == "/board" {
            show_board(board).await?;
            continue;
        }

        if let Some(task_title) = input.strip_prefix("/task ") {
            let task_title = task_title.trim().trim_matches('"');
            if task_title.is_empty() {
                println!("Usage: /task \"task description\"");
                continue;
            }
            handle_task(board, agent, session_id, task_title).await;
            continue;
        }

        // 일반 대화
        run_agent_streaming(agent, session_id, input).await;
        println!();
    }

    println!("\nBye!");
    Ok(())
}

// ── /task 명령 ───────────────────────────────────────────────

async fn handle_task(board: &DbBoard, agent: &Agent, session_id: &str, title: &str) {
    let rig_id = RigId::new("main");

    let item = match board
        .post(PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: rig_id.clone(),
            priority: Priority::P1,
        })
        .await
    {
        Ok(item) => item,
        Err(e) => {
            eprintln!("Post failed: {e}");
            return;
        }
    };

    println!("● #{} \"{}\" — posted", item.id, item.title);

    match board.claim(item.id, &rig_id).await {
        Ok(_) => println!("  → claimed by {rig_id}"),
        Err(e) => {
            eprintln!("  → claim failed: {e}");
            return;
        }
    }

    let prompt = format!(
        "You have claimed work item #{}: \"{}\". Complete this task. \
         When done, run: opengoose board submit {}",
        item.id, item.title, item.id
    );
    run_agent_streaming(agent, session_id, &prompt).await;

    // Agent가 `opengoose board submit`을 실행했을 수도 있지만, 안전망으로 직접 submit 시도
    match board.submit(item.id, &rig_id).await {
        Ok(_) => println!("\n✓ #{} completed", item.id),
        Err(_) => {
            // 이미 submit 됐거나 다른 상태 — 현재 상태 확인
            if let Ok(Some(current)) = board.get(item.id).await {
                if current.status == Status::Done {
                    println!("\n✓ #{} completed (by agent)", item.id);
                } else {
                    println!("\nℹ #{} status: {:?}", item.id, current.status);
                }
            }
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
    board: &DbBoard,
    agent: &Agent,
    session_id: &str,
    task: &str,
) -> Result<()> {
    let rig_id = RigId::new("main");
    board
        .post(PostWorkItem {
            title: task.to_string(),
            description: String::new(),
            created_by: rig_id,
            priority: Priority::P1,
        })
        .await?;

    run_agent_streaming(agent, session_id, task).await;
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
