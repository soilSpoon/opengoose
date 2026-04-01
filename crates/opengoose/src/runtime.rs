// Runtime init + agent creation — Board, web server, Evolver, WorkerPool wiring

use anyhow::Result;
use goose::agents::Agent;
use opengoose_board::Board;
use opengoose_rig::pipeline::{ContextHydrator, Middleware, ValidationGate};
use std::sync::Arc;

use crate::web;
use crate::worker_pool::WorkerPool;

/// Encapsulates the Board + WorkerPool handles created during runtime init.
pub struct Runtime {
    pub board: Arc<Board>,
    pub workers: Arc<WorkerPool>,
}

/// Stand up the full runtime: Board, web dashboard, Evolver, and WorkerPool.
pub async fn init_runtime(port: u16, sandbox: bool, num_workers: u16) -> Result<Runtime> {
    let board = Arc::new(Board::connect(&crate::db_url()).await?);

    // Evolver
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(opengoose_evolver::run(Arc::clone(&board), stamp_notify));

    // Validation middleware — sandbox VM or host
    let validation: Arc<dyn Middleware> = if sandbox {
        #[cfg(target_os = "macos")]
        {
            let pool = Arc::new(opengoose_sandbox::SandboxPool::new());
            Arc::new(crate::sandbox_gate::SandboxValidationGate::new(pool))
        }
        #[cfg(not(target_os = "macos"))]
        {
            tracing::warn!("--sandbox is only supported on macOS, falling back to host validation");
            Arc::new(ValidationGate)
        }
    } else {
        Arc::new(ValidationGate)
    };

    let middleware: Vec<Arc<dyn Middleware>> = vec![
        Arc::new(ContextHydrator {
            skill_catalog: String::new(),
        }),
        validation,
    ];
    let workers = Arc::new(WorkerPool::new(Arc::clone(&board), middleware));

    // Web dashboard (needs workers reference)
    web::spawn_server(Arc::clone(&board), port, Arc::clone(&workers)).await?;

    // Spawn initial workers
    for _ in 0..num_workers {
        if let Err(e) = workers.spawn(None, Default::default()).await {
            tracing::warn!(error = %e, "initial worker creation failed");
        }
    }

    Ok(Runtime { board, workers })
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
