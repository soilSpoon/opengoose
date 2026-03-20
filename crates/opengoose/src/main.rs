// OpenGoose v0.2 — CLI 진입점
//
// 기본: ratatui TUI. 서브커맨드 있으면 headless CLI.
// Board + Goose Agent를 와이어링. 모든 작업이 Board를 통과.

mod evolver;
mod logs;
mod tui;
mod web;
mod skills;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use goose::model::ModelConfig;
use goose::session::session_manager::SessionType;
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
use std::io::{self, Write};
use std::sync::Arc;
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

    /// 웹 대시보드 포트
    #[arg(long, default_value = "1355", global = true)]
    port: u16,
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
    /// Rig 관리
    Rigs {
        #[command(subcommand)]
        action: Option<RigsAction>,
    },
    /// Skill 관리
    Skills {
        #[command(subcommand)]
        action: skills::SkillsAction,
    },
    /// 대화 로그 관리
    Logs {
        #[command(subcommand)]
        action: logs::LogsAction,
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
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// 작업 포기
    Abandon { id: i64 },
    /// 작업 평가 (stamp)
    Stamp {
        /// 작업 ID
        id: i64,
        #[arg(long, short = 'q')]
        quality: f32,
        #[arg(long, short = 'r')]
        reliability: f32,
        #[arg(long, short = 'p')]
        helpfulness: f32,
        #[arg(long, default_value = "Leaf")]
        severity: String,
        /// 선택적 코멘트
        #[arg(long)]
        comment: Option<String>,
    },
}

#[derive(Subcommand)]
enum RigsAction {
    /// AI rig 추가
    Add {
        #[arg(long)]
        id: String,
        #[arg(long)]
        recipe: String,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Rig 제거
    Remove { id: String },
    /// Rig 신뢰 수준 조회
    Trust { id: String },
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
            let board = Board::connect(&db_url()).await?;
            run_board_command(&board, action).await
        }
        Some(Commands::Rigs { action }) => {
            let board = Board::connect(&db_url()).await?;
            run_rigs_command(&board, action).await
        }
        Some(Commands::Skills { action }) => {
            skills::run_skills_command(action).await
        }
        Some(Commands::Logs { action }) => {
            logs::run_logs_command(action)
        }
        Some(Commands::Run { task }) => {
            let board = Arc::new(Board::connect(&db_url()).await?);
            web::spawn_server(Arc::clone(&board), cli.port).await?;
            // Spawn Evolver
            let stamp_notify = board.stamp_notify_handle();
            tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));
            let (agent, session_id) = create_operator_agent().await?;
            run_headless(&board, &agent, &session_id, &task).await
        }
        None => {
            let board = Arc::new(Board::connect(&db_url()).await?);
            web::spawn_server(Arc::clone(&board), cli.port).await?;
            // Spawn Evolver
            let stamp_notify = board.stamp_notify_handle();
            tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));
            // Spawn Worker
            let (worker_agent, _) = create_worker_agent().await?;
            let worker = Arc::new(opengoose_rig::rig::Worker::new(
                RigId::new("worker"),
                Arc::clone(&board),
                worker_agent,
                opengoose_rig::work_mode::TaskMode,
            ));
            let worker_handle = Arc::clone(&worker);
            tokio::spawn(async move { worker_handle.run().await });
            // Operator
            let (agent, session_id) = create_operator_agent().await?;
            let agent = Arc::new(agent);
            let result = tui::run_tui(board, agent, session_id).await;
            worker.cancel();
            result
        }
    }
}

// ── Board CLI ────────────────────────────────────────────────

async fn run_board_command(board: &Board, action: BoardAction) -> Result<()> {
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
        BoardAction::Create { title, priority, tags } => {
            let priority = Priority::parse(&priority).unwrap_or_default();
            let item = board
                .post(PostWorkItem {
                    title,
                    description: String::new(),
                    created_by: rig_id,
                    priority,
                    tags,
                })
                .await?;
            println!("Created #{}: \"{}\" ({:?})", item.id, item.title, item.priority);
        }
        BoardAction::Abandon { id } => {
            let item = board.abandon(id).await?;
            println!("Abandoned #{}: \"{}\"", item.id, item.title);
        }
        BoardAction::Stamp {
            id,
            quality,
            reliability,
            helpfulness,
            severity,
            comment,
        } => {
            let stamped_by = "human";
            // 작업의 claimed_by가 target rig
            let item = board.get(id).await?.ok_or_else(|| anyhow::anyhow!("item not found"))?;
            let target = item
                .claimed_by
                .as_ref()
                .map(|r| r.0.as_str())
                .unwrap_or(&item.created_by.0);

            let comment_ref = comment.as_deref();
            for (dim, score) in [("Quality", quality), ("Reliability", reliability), ("Helpfulness", helpfulness)] {
                board.add_stamp(target, id, dim, score, &severity, stamped_by, comment_ref, None).await?;
            }

            let trust = board.trust_level(target).await?;
            let pts = board.weighted_score(target).await?;
            println!("Stamped #{id} (target: {target}): q:{quality} r:{reliability} h:{helpfulness} {severity}");
            if let Some(c) = &comment {
                println!("  comment: {c}");
            }
            println!("  {target}: {trust} ({pts:.1}pts)");

            // Evolver run loop handles skill generation from low stamps asynchronously
        }
    }

    Ok(())
}

async fn show_board(board: &Board) -> Result<()> {
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

// ── Rigs CLI ─────────────────────────────────────────────────

async fn run_rigs_command(board: &Board, action: Option<RigsAction>) -> Result<()> {
    match action {
        None => {
            // opengoose rigs — 목록 표시
            let rigs = board.list_rigs().await?;
            if rigs.is_empty() {
                println!("No rigs registered.");
            } else {
                for rig in &rigs {
                    let tags = rig.tags.as_deref().unwrap_or("[]");
                    let recipe = rig.recipe.as_deref().unwrap_or("-");
                    println!(
                        "  {}  {}  recipe:{}  tags:{}",
                        rig.id, rig.rig_type, recipe, tags
                    );
                }
            }
        }
        Some(RigsAction::Add { id, recipe, tags }) => {
            let tags = if tags.is_empty() { None } else { Some(tags.as_slice()) };
            board.register_rig(&id, "ai", Some(&recipe), tags).await?;
            println!("Registered {id} (recipe: {recipe})");
        }
        Some(RigsAction::Remove { id }) => {
            board.remove_rig(&id).await?;
            println!("Removed {id}");
        }
        Some(RigsAction::Trust { id }) => {
            let pts = board.weighted_score(&id).await?;
            let level = board.trust_level(&id).await?;
            println!("{id}: {level} ({pts:.1}pts)");
        }
    }
    Ok(())
}

// ── Agent 생성 ───────────────────────────────────────────────

async fn create_base_agent(session_name: &str) -> Result<(Agent, String)> {
    let provider_name =
        std::env::var("GOOSE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());

    let agent = Agent::new();

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let session = agent
        .config
        .session_manager
        .create_session(cwd, session_name.into(), SessionType::User)
        .await
        .context("failed to create session")?;

    let provider = match std::env::var("GOOSE_MODEL") {
        Ok(model_name) => {
            info!(provider = %provider_name, model = %model_name, session = %session_name, "creating agent");
            let model_config = ModelConfig::new(&model_name)
                .context("invalid model config")?
                .with_canonical_limits(&provider_name);
            goose::providers::create(&provider_name, model_config, vec![]).await
        }
        Err(_) => {
            info!(provider = %provider_name, model = "default", session = %session_name, "creating agent");
            goose::providers::create_with_default_model(&provider_name, vec![]).await
        }
    }
    .context("failed to create provider")?;

    agent
        .update_provider(provider, &session.id)
        .await
        .context("failed to set provider")?;

    Ok((agent, session.id))
}

async fn create_operator_agent() -> Result<(Agent, String)> {
    let (agent, session_id) = create_base_agent("opengoose").await?;
    agent
        .extend_system_prompt(
            "opengoose".to_string(),
            "You are an OpenGoose Operator rig — you handle interactive conversation.\n\
             A separate Worker rig automatically claims and executes Board tasks.\n\n\
             Available commands (run via shell):\n\
             - opengoose board status    — show board state (open/claimed/done)\n\
             - opengoose board ready     — list claimable work items\n\
             - opengoose board create \"TITLE\" — post a new task\n\
             \n\
             When the user posts a task via /task, it goes to the Board and the Worker picks it up automatically.\n\
             You do NOT need to claim or submit tasks yourself."
                .to_string(),
        )
        .await;
    Ok((agent, session_id))
}

async fn create_worker_agent() -> Result<(Agent, String)> {
    let (agent, session_id) = create_base_agent("worker").await?;
    agent
        .extend_system_prompt(
            "worker".to_string(),
            "You are an OpenGoose Worker rig. You receive tasks from the Board and execute them autonomously.\n\
             Focus on completing the task. Use available tools. Do not ask clarifying questions — make reasonable assumptions and proceed."
                .to_string(),
        )
        .await;
    Ok((agent, session_id))
}

// ── Agent 스트리밍 실행 (headless 전용) ──────────────────────

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
                match event {
                    Ok(AgentEvent::Message(msg)) => print_message(&msg),
                    Err(e) => {
                        eprintln!("\nStream error: {e}");
                        break;
                    }
                    _ => {}
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
    board: &Board,
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
            tags: vec![],
        })
        .await?;

    tokio::select! {
        _ = run_agent_streaming(agent, session_id, task) => {}
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nInterrupted.");
        }
    }
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
