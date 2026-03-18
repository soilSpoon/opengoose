// OpenGoose v0.2 — CLI 진입점
//
// 대화형 REPL + 헤드리스 `run` 모드.
// Board + Rig + Goose Agent를 와이어링하는 유일한 장소.

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
use std::io::{Write, self};
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
                .unwrap_or_else(|_| "opengoose=info,opengoose_rig=info,warn".into()),
        )
        .init();

    let cli = Cli::parse();

    // Board + Agent 생성
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

/// Goose Agent 생성 + Provider 설정 + Board 도구 주입. (Agent, session_id) 반환.
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

    // GOOSE_MODEL 지정 시 사용, 없으면 provider 기본 모델
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

    // 시스템 프롬프트 확장
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

/// 헤드리스 모드: 단일 작업 실행 후 종료.
async fn run_headless(
    board: Arc<Mutex<Board>>,
    rig_id: RigId,
    agent: Agent,
    session_id: &str,
    task: &str,
) -> Result<()> {
    // 작업을 Board에 게시
    {
        let mut b = board.lock().await;
        b.post(PostWorkItem {
            title: task.to_string(),
            description: String::new(),
            created_by: rig_id.clone(),
            priority: Priority::P1,
        });
    }

    let message = Message::user().with_text(task);
    let session_config = SessionConfig {
        id: session_id.to_string(),
        schedule_id: None,
        max_turns: None,
        retry_config: None,
    };

    let stream = agent.reply(message, session_config, None).await?;
    tokio::pin!(stream);

    while let Some(event) = stream.next().await {
        if let Ok(AgentEvent::Message(msg)) = event {
            print_message(&msg);
        }
    }

    println!();
    Ok(())
}

/// 대화형 REPL.
async fn run_repl(
    board: Arc<Mutex<Board>>,
    rig_id: RigId,
    agent: Agent,
    session_id: &str,
) -> Result<()> {
    println!("OpenGoose v0.2 (Ctrl+D to exit)");
    println!();

    loop {
        // 프롬프트
        print!("> ");
        io::stdout().flush()?;

        // 입력 읽기
        let mut input = String::new();
        let bytes = io::stdin().read_line(&mut input)?;
        if bytes == 0 {
            break; // EOF (Ctrl+D)
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Board에 게시
        {
            let mut b = board.lock().await;
            b.post(PostWorkItem {
                title: input.to_string(),
                description: String::new(),
                created_by: rig_id.clone(),
                priority: Priority::P1,
            });
        }

        // Agent 실행
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
                println!();
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
    }

    println!("\nBye!");
    Ok(())
}

/// AgentEvent::Message에서 텍스트 추출하여 출력.
fn print_message(msg: &Message) {
    use goose::conversation::message::MessageContent;
    // rmcp::model::Role은 pub — assistant 메시지만 출력
    if msg.role == rmcp::model::Role::Assistant {
        for content in &msg.content {
            if let MessageContent::Text(text) = content {
                print!("{}", text.text);
                io::stdout().flush().ok();
            }
        }
    }
}

