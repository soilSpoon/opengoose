// OpenGoose v0.2 — CLI 진입점
//
// 기본: ratatui TUI. 서브커맨드 있으면 headless CLI.
// Board + Goose Agent를 와이어링. 모든 작업이 Board를 통과.

mod cli;
mod commands;
mod evolver;
mod logs;
mod runtime;
mod skills;
mod tui;
mod web;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
use opengoose_rig::pipeline::{ContextHydrator, ValidationGate};
use std::sync::Arc;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Global mutex for tests that modify environment variables (HOME, XDG_STATE_HOME, cwd).
/// All such tests across every module must acquire this lock to avoid cross-contamination.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Return the user's home directory, preferring $HOME (for test isolation)
/// and falling back to `dirs::home_dir()`.
pub(crate) fn home_dir() -> std::path::PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        std::path::PathBuf::from(h)
    } else {
        dirs::home_dir().unwrap_or_else(|| ".".into())
    }
}

fn db_url() -> String {
    let home = home_dir();
    let dir = home.join(".opengoose");
    std::fs::create_dir_all(&dir).ok();
    format!("sqlite://{}?mode=rwc", dir.join("board.db").display())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let log_rx = match &cli.command {
        None => {
            // TUI 모드: 파일 + TuiLayer (stderr 없음)
            let log_file = tui::log_entry::create_session_log_file()?;
            tui::log_entry::cleanup_old_logs(10)?;
            let (log_tx, log_rx) = tokio::sync::mpsc::channel::<tui::log_entry::LogEntry>(1000);
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer().with_writer(std::sync::Mutex::new(log_file)))
                .with(tui::tui_layer::TuiLayer::new(log_tx))
                .with(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "opengoose=info,goose=error".into()),
                )
                .init();
            Some(log_rx)
        }
        Some(Commands::Run { .. }) => {
            // Headless: stderr + 파일
            let log_file = tui::log_entry::create_session_log_file()?;
            tui::log_entry::cleanup_old_logs(10)?;
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
                .with(tracing_subscriber::fmt::layer().with_writer(std::sync::Mutex::new(log_file)))
                .with(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "opengoose=info,goose=error".into()),
                )
                .init();
            None
        }
        _ => {
            // CLI 서브커맨드: stderr만
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "opengoose=info,goose=error".into()),
                )
                .init();
            None
        }
    };

    match cli.command {
        Some(Commands::Board { action }) => {
            let board = Board::connect(&db_url()).await?;
            commands::board::run_board_command(&board, action).await
        }
        Some(Commands::Rigs { action }) => {
            let board = Board::connect(&db_url()).await?;
            commands::rigs::run_rigs_command(&board, action).await
        }
        Some(Commands::Skills { action }) => skills::run_skills_command(action).await,
        Some(Commands::Logs { action }) => logs::run_logs_command(action),
        Some(Commands::Run { task }) => {
            let rt = init_runtime(cli.port).await?;
            let result = run_headless(&rt.board, &task).await;
            rt.worker.cancel();
            result
        }
        None => {
            let log_rx = log_rx.expect("TUI mode must have log_rx");
            let rt = init_runtime(cli.port).await?;
            let (agent, session_id) = runtime::create_operator_agent().await?;
            let operator = Arc::new(opengoose_rig::rig::Operator::without_board(
                RigId::new("operator"),
                agent,
                &session_id,
            ));
            let result = tui::run_tui(rt.board, operator, log_rx).await;
            rt.worker.cancel();
            result
        }
    }
}

// ── Runtime ──────────────────────────────────────────────────

struct Runtime {
    board: Arc<Board>,
    worker: Arc<opengoose_rig::rig::Worker>,
}

async fn init_runtime(port: u16) -> Result<Runtime> {
    let board = Arc::new(Board::connect(&db_url()).await?);
    web::spawn_server(Arc::clone(&board), port).await?;

    // Evolver
    let stamp_notify = board.stamp_notify_handle();
    tokio::spawn(evolver::run(Arc::clone(&board), stamp_notify));

    // Worker
    let (worker_agent, _) = runtime::create_worker_agent().await?;
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

// ── 헤드리스 모드 ────────────────────────────────────────────

async fn run_headless(board: &Board, task: &str) -> Result<()> {
    let rig_id = RigId::new("headless");
    let item = board
        .post(PostWorkItem {
            title: task.to_string(),
            description: String::new(),
            created_by: rig_id,
            priority: Priority::P1,
            tags: vec![],
        })
        .await?;

    println!(
        "Posted #{}: \"{}\" — waiting for Worker...",
        item.id, item.title
    );

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(600));
    tokio::pin!(timeout);

    loop {
        let notify = board.notify_handle();
        let notified = notify.notified();

        match board.get(item.id).await? {
            Some(wi) if wi.status == Status::Done => {
                println!("✓ #{} completed", item.id);
                break;
            }
            Some(wi) if wi.status == Status::Abandoned => {
                anyhow::bail!("work item #{} was abandoned", item.id);
            }
            Some(_) => {}
            None => anyhow::bail!("work item #{} was deleted", item.id),
        }

        tokio::select! {
            _ = notified => {}
            _ = &mut timeout => anyhow::bail!("timed out waiting for work item #{} (10 min)", item.id),
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nInterrupted.");
                return Ok(());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use clap::Parser;
    use cli::{BoardAction, RigsAction};
    use commands::board::run_board_command;
    use commands::rigs::run_rigs_command;
    use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
    use std::sync::Arc;

    #[test]
    fn db_url_points_to_board_db() {
        let url = db_url();
        assert!(url.starts_with("sqlite://"));
        assert!(url.ends_with(".opengoose/board.db?mode=rwc"));
    }

    #[test]
    fn parse_board_status_command() {
        let cli = Cli::parse_from(["opengoose", "--port", "1355", "board", "status"]);
        assert_eq!(cli.port, 1355);
        match cli.command {
            Some(Commands::Board {
                action: BoardAction::Status,
            }) => {}
            _ => panic!("unexpected command"),
        }
    }

    async fn new_board() -> Arc<Board> {
        Arc::new(Board::in_memory().await.unwrap())
    }

    #[tokio::test]
    async fn run_board_command_status_smoke() {
        let board = new_board().await;
        board
            .post(PostWorkItem {
                title: "Task".into(),
                description: String::new(),
                created_by: RigId::new("creator"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        run_board_command(&board, BoardAction::Status)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_board_command_ready_and_create() {
        let board = new_board().await;
        run_board_command(&board, BoardAction::Ready).await.unwrap();
        run_board_command(
            &board,
            BoardAction::Create {
                title: "new task".into(),
                priority: "P1".into(),
                tags: vec!["ui".into()],
            },
        )
        .await
        .unwrap();
        let items = board.list().await.unwrap();
        assert_eq!(items.len(), 1);
        run_board_command(&board, BoardAction::Ready).await.unwrap();
    }

    #[tokio::test]
    async fn run_board_command_claim_submit_abandon_stamp() {
        let board = new_board().await;
        let open = board
            .post(PostWorkItem {
                title: "claimable".into(),
                description: String::new(),
                created_by: RigId::new("creator"),
                priority: Priority::P2,
                tags: vec![],
            })
            .await
            .unwrap();

        run_board_command(&board, BoardAction::Claim { id: open.id })
            .await
            .unwrap();

        let claimed = board
            .get(open.id)
            .await
            .unwrap()
            .expect("claimed item should exist");
        assert_eq!(claimed.status, Status::Claimed);

        board
            .post(PostWorkItem {
                title: "cleanup".into(),
                description: String::new(),
                created_by: RigId::new("creator"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        run_board_command(&board, BoardAction::Submit { id: open.id })
            .await
            .unwrap();

        let done = board
            .get(open.id)
            .await
            .unwrap()
            .expect("done item should exist");
        assert_eq!(done.status, Status::Done);

        let abandon = board
            .post(PostWorkItem {
                title: "abandon".into(),
                description: String::new(),
                created_by: RigId::new("creator"),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .unwrap();
        run_board_command(&board, BoardAction::Abandon { id: abandon.id })
            .await
            .unwrap();

        run_board_command(
            &board,
            BoardAction::Stamp {
                id: done.id,
                quality: 0.2,
                reliability: 0.3,
                helpfulness: 0.4,
                severity: "Leaf".into(),
                comment: Some("minor delay".into()),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_rigs_command_cycle() {
        let board = new_board().await;
        run_rigs_command(&board, None).await.unwrap();

        run_rigs_command(
            &board,
            Some(RigsAction::Add {
                id: "worker-01".into(),
                recipe: "small".into(),
                tags: vec!["tag".into()],
            }),
        )
        .await
        .unwrap();
        assert!(board.get_rig("worker-01").await.unwrap().is_some());

        run_rigs_command(
            &board,
            Some(RigsAction::Trust {
                id: "worker-01".into(),
            }),
        )
        .await
        .unwrap();

        run_rigs_command(
            &board,
            Some(RigsAction::Remove {
                id: "worker-01".into(),
            }),
        )
        .await
        .unwrap();
        assert!(board.get_rig("worker-01").await.unwrap().is_none());
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]
    use super::*;
    use crate::skills::test_env_lock;
    use cli::{BoardAction, RigsAction};
    use commands::board::{run_board_command, show_board};
    use commands::rigs::run_rigs_command;
    use opengoose_board::work_item::{PostWorkItem, Priority, RigId};
    use runtime::{AgentConfig, create_agent};
    use std::ffi::OsString;
    use tempfile::tempdir;

    #[test]
    fn db_url_points_to_home_opengoose() {
        let url = db_url();
        assert!(url.starts_with("sqlite://"));
        assert!(url.ends_with("board.db?mode=rwc"));
    }

    #[test]
    fn home_dir_uses_home_env_var() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", "/tmp/test-home-dir");
        }
        let result = home_dir();
        assert_eq!(result, std::path::PathBuf::from("/tmp/test-home-dir"));
        unsafe {
            match prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[tokio::test]
    async fn run_board_command_stamp_with_no_comment() {
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let item = board
            .post(PostWorkItem {
                title: "stamp no comment".into(),
                description: String::new(),
                created_by: RigId::new("tester"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        run_board_command(
            &board,
            BoardAction::Stamp {
                id: item.id,
                quality: 0.9,
                reliability: 0.8,
                helpfulness: 0.7,
                severity: "Leaf".into(),
                comment: None,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_board_command_create_with_invalid_priority_uses_default() {
        let board = Board::connect("sqlite::memory:").await.unwrap();
        run_board_command(
            &board,
            BoardAction::Create {
                title: "invalid priority task".into(),
                priority: "INVALID".into(),
                tags: vec![],
            },
        )
        .await
        .unwrap();
        let items = board.list().await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].priority, Priority::P1); // default
    }

    #[tokio::test]
    async fn run_board_command_covers_action_branches() {
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let claimer = RigId::new("cli");

        run_board_command(
            &board,
            BoardAction::Create {
                title: "task alpha".into(),
                priority: "P2".into(),
                tags: vec!["urgent".into()],
            },
        )
        .await
        .unwrap();

        let created = board
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|item| item.title == "task alpha")
            .unwrap()
            .id;

        run_board_command(&board, BoardAction::Status)
            .await
            .unwrap();
        run_board_command(&board, BoardAction::Ready).await.unwrap();
        run_board_command(&board, BoardAction::Claim { id: created })
            .await
            .unwrap();
        run_board_command(&board, BoardAction::Submit { id: created })
            .await
            .unwrap();

        let stamp_target = board
            .post(PostWorkItem {
                title: "stamp target".into(),
                description: String::new(),
                created_by: RigId::new("tester"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap()
            .id;

        run_board_command(
            &board,
            BoardAction::Stamp {
                id: stamp_target,
                quality: 0.8,
                reliability: 0.7,
                helpfulness: 0.6,
                severity: "Leaf".into(),
                comment: Some("good".into()),
            },
        )
        .await
        .unwrap();

        let abandon_target = board
            .post(PostWorkItem {
                title: "task beta".into(),
                description: String::new(),
                created_by: claimer.clone(),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .unwrap()
            .id;
        run_board_command(&board, BoardAction::Abandon { id: abandon_target })
            .await
            .unwrap();

        run_board_command(&board, BoardAction::Status)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn run_board_command_covers_empty_and_mixed_status_states() {
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let claimer = RigId::new("mixed-claimer");
        let tester = RigId::new("tester");

        run_board_command(&board, BoardAction::Status)
            .await
            .unwrap();
        run_board_command(&board, BoardAction::Ready).await.unwrap();

        let open_item = board
            .post(PostWorkItem {
                title: "open item".into(),
                description: String::new(),
                created_by: tester.clone(),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        let claimed_source = board
            .post(PostWorkItem {
                title: "claimed item".into(),
                description: String::new(),
                created_by: tester.clone(),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .unwrap();

        board.claim(open_item.id, &claimer).await.unwrap();
        board.claim(claimed_source.id, &claimer).await.unwrap();
        board.submit(open_item.id, &claimer).await.unwrap();

        run_board_command(&board, BoardAction::Status)
            .await
            .unwrap();
        run_board_command(&board, BoardAction::Ready).await.unwrap();
    }

    #[tokio::test]
    async fn run_rigs_command_covers_add_list_remove_trust() {
        let board = Board::connect("sqlite::memory:").await.unwrap();

        run_rigs_command(
            &board,
            Some(RigsAction::Add {
                id: "r-test".into(),
                recipe: "echo recipe".into(),
                tags: vec!["fast".into()],
            }),
        )
        .await
        .unwrap();

        run_rigs_command(&board, None).await.unwrap();
        run_rigs_command(
            &board,
            Some(RigsAction::Trust {
                id: "r-test".into(),
            }),
        )
        .await
        .unwrap();
        run_rigs_command(
            &board,
            Some(RigsAction::Remove {
                id: "r-test".into(),
            }),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_rigs_command_covers_empty_and_list_branches() {
        let board = Board::connect("sqlite::memory:").await.unwrap();

        run_rigs_command(&board, None).await.unwrap();

        run_rigs_command(
            &board,
            Some(RigsAction::Add {
                id: "r-empty".into(),
                recipe: "echo empty".into(),
                tags: vec![],
            }),
        )
        .await
        .unwrap();

        run_rigs_command(&board, None).await.unwrap();

        run_rigs_command(
            &board,
            Some(RigsAction::Trust {
                id: "r-empty".into(),
            }),
        )
        .await
        .unwrap();

        run_rigs_command(
            &board,
            Some(RigsAction::Remove {
                id: "r-empty".into(),
            }),
        )
        .await
        .unwrap();
    }

    fn set_env_var(key: &str, value: Option<&str>) -> Option<OsString> {
        let prev = std::env::var_os(key);
        unsafe {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        prev
    }

    fn restore_env_var(key: &str, prev: Option<OsString>) {
        unsafe {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn create_agent_rejects_invalid_provider() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let workdir = tempdir().unwrap();
        let prev_home = set_env_var("HOME", workdir.path().to_str());
        let prev_provider = set_env_var("GOOSE_PROVIDER", Some("invalid-provider"));
        let prev_model = set_env_var("GOOSE_MODEL", None);

        let result = create_agent(AgentConfig {
            session_id: "opengoose".into(),
            system_prompt: None,
        })
        .await;
        assert!(result.is_err());

        restore_env_var("HOME", prev_home);
        restore_env_var("GOOSE_PROVIDER", prev_provider);
        restore_env_var("GOOSE_MODEL", prev_model);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn create_agent_rejects_invalid_model() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let workdir = tempdir().unwrap();
        let prev_home = set_env_var("HOME", workdir.path().to_str());
        let prev_provider = set_env_var("GOOSE_PROVIDER", Some("anthropic"));
        let prev_model = set_env_var("GOOSE_MODEL", Some("??invalid-model??"));

        let result = create_agent(AgentConfig {
            session_id: "worker".into(),
            system_prompt: None,
        })
        .await;
        assert!(result.is_err());

        restore_env_var("HOME", prev_home);
        restore_env_var("GOOSE_PROVIDER", prev_provider);
        restore_env_var("GOOSE_MODEL", prev_model);
    }

    #[tokio::test]
    async fn run_headless_times_out_when_no_worker_claims_task() {
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            run_headless(&board, "solve test"),
        )
        .await;
        assert!(result.is_err());

        let items = board.list().await.unwrap();
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn show_board_with_many_done_items_takes_5() {
        let board = Board::connect("sqlite::memory:").await.unwrap();
        let claimer = RigId::new("claimer");
        for i in 0..7 {
            let item = board
                .post(PostWorkItem {
                    title: format!("task {i}"),
                    description: String::new(),
                    created_by: RigId::new("tester"),
                    priority: Priority::P1,
                    tags: vec![],
                })
                .await
                .unwrap();
            board.claim(item.id, &claimer).await.unwrap();
            board.submit(item.id, &claimer).await.unwrap();
        }
        show_board(&board).await.unwrap();
    }

    #[tokio::test]
    async fn run_headless_completes_when_worker_submits() {
        let board = Arc::new(Board::connect("sqlite::memory:").await.unwrap());
        let board2 = Arc::clone(&board);

        // Spawn a worker that submits the item once it's posted
        let worker = tokio::spawn(async move {
            // Give run_headless time to post the item
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            loop {
                let items = board2.list().await.unwrap();
                if let Some(item) = items.iter().find(|i| i.status == Status::Open) {
                    board2.claim(item.id, &RigId::new("worker")).await.ok();
                    board2.submit(item.id, &RigId::new("worker")).await.ok();
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        });

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_headless(&board, "complete this task"),
        )
        .await;
        assert!(result.is_ok(), "run_headless should complete");
        assert!(result.unwrap().is_ok());
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn run_headless_bails_when_item_abandoned() {
        let board = Arc::new(Board::connect("sqlite::memory:").await.unwrap());
        let board2 = Arc::clone(&board);

        let worker = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            loop {
                let items = board2.list().await.unwrap();
                if let Some(item) = items.iter().find(|i| i.status == Status::Open) {
                    board2.abandon(item.id).await.ok();
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        });

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_headless(&board, "abandon this task"),
        )
        .await;
        assert!(result.is_ok(), "should not time out");
        assert!(result.unwrap().is_err(), "should bail on abandoned");
        worker.await.unwrap();
    }
}
