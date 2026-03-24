// Runtime init + agent creation — Board, web server, Evolver, Worker wiring

use anyhow::{Context, Result};
use goose::agents::Agent;
use goose::model::ModelConfig;
use goose::session::session_manager::SessionType;
use opengoose_board::Board;
use opengoose_board::work_item::RigId;
use opengoose_rig::pipeline::{ContextHydrator, ValidationGate};
use std::sync::Arc;
use tracing::info;

use crate::{evolver, web};

/// Encapsulates the Board + Worker handles created during runtime init.
pub struct Runtime {
    pub board: Arc<Board>,
    pub worker: Arc<opengoose_rig::rig::Worker>,
}

/// Stand up the full runtime: Board, web dashboard, Evolver, and Worker.
pub async fn init_runtime(port: u16) -> Result<Runtime> {
    let board = Arc::new(Board::connect(&crate::db_url()).await?);
    web::spawn_server(Arc::clone(&board), port).await?;

    // Evolver
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));

    // Worker
    let (worker_agent, _) = create_worker_agent().await?;
    let worker = Arc::new(opengoose_rig::rig::Worker::new(
        RigId::new("worker"),
        Arc::clone(&board),
        worker_agent,
        opengoose_rig::work_mode::TaskMode,
        vec![
            Arc::new(ContextHydrator {
                skill_catalog: String::new(),
            }),
            Arc::new(ValidationGate),
        ],
    ));
    let worker_handle = Arc::clone(&worker);
    tokio::spawn(async move { worker_handle.run().await });

    Ok(Runtime { board, worker })
}

pub struct AgentConfig {
    pub session_id: String,
    pub system_prompt: Option<String>,
}

/// Create a Goose Agent with the given config.
/// Reads GOOSE_PROVIDER and GOOSE_MODEL from the environment.
pub async fn create_agent(config: AgentConfig) -> Result<Agent> {
    let provider_name = std::env::var("GOOSE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());

    let agent = Agent::new();

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let session = agent
        .config
        .session_manager
        .create_session(
            cwd,
            config.session_id.clone(),
            SessionType::User,
            goose::config::goose_mode::GooseMode::Auto,
        )
        .await
        .context("failed to create session")?;

    let provider = match std::env::var("GOOSE_MODEL") {
        Ok(model_name) => {
            info!(
                provider = %provider_name,
                model = %model_name,
                session = %config.session_id,
                "creating agent"
            );
            let model_config = ModelConfig::new(&model_name)
                .context("invalid model config")?
                .with_canonical_limits(&provider_name);
            goose::providers::create(&provider_name, model_config, vec![]).await
        }
        Err(_) => {
            info!(
                provider = %provider_name,
                model = "default",
                session = %config.session_id,
                "creating agent"
            );
            goose::providers::create_with_default_model(&provider_name, vec![]).await
        }
    }
    .context("failed to create provider")?;

    agent
        .update_provider(provider, &session.id)
        .await
        .context("failed to set provider")?;

    if let Some(prompt) = config.system_prompt {
        agent
            .extend_system_prompt(config.session_id.clone(), prompt)
            .await;
    }

    Ok(agent)
}

/// Create an Operator agent (interactive conversation).
pub async fn create_operator_agent() -> Result<(Agent, String)> {
    let session_name = "opengoose".to_string();
    let agent = create_agent(AgentConfig {
        session_id: session_name.clone(),
        system_prompt: Some(
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
        ),
    })
    .await?;
    Ok((agent, session_name))
}

/// Create a Worker agent (autonomous task execution).
pub async fn create_worker_agent() -> Result<(Agent, String)> {
    let session_name = "worker".to_string();
    let agent = create_agent(AgentConfig {
        session_id: session_name.clone(),
        system_prompt: Some(
            "You are an OpenGoose Worker rig. You receive tasks from the Board and execute them autonomously.\n\
             Focus on completing the task. Use available tools. Do not ask clarifying questions — make reasonable assumptions and proceed."
                .to_string(),
        ),
    })
    .await?;
    Ok((agent, session_name))
}
