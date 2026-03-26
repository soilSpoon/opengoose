// Runtime init + agent creation — Board, web server, Evolver, Worker wiring

use anyhow::Result;
use goose::agents::Agent;
use opengoose_board::Board;
use opengoose_board::work_item::RigId;
use opengoose_rig::pipeline::{ContextHydrator, ValidationGate};
use std::sync::Arc;

use crate::web;

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
    tokio::spawn(opengoose_evolver::run(Arc::clone(&board), stamp_notify));

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

pub use opengoose_rig::agent_factory::{AgentConfig, create_agent};

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
