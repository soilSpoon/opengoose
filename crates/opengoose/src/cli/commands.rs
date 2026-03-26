// Subcommand dispatch — routes CLI commands to their handlers

use anyhow::Result;
use opengoose_board::Board;
use opengoose_board::work_item::RigId;
use std::sync::Arc;

use super::{Cli, Commands};
use crate::cli::setup::db_url;
use crate::tui::log_entry::LogEntry;
use tokio::sync::mpsc::Receiver;

/// Dispatch a parsed CLI to the appropriate subcommand handler.
pub async fn dispatch(cli: Cli, log_rx: Option<Receiver<LogEntry>>) -> Result<()> {
    match cli.command {
        Some(Commands::Board { action }) => {
            let board = Board::connect(&db_url()).await?;
            crate::commands::board::run_board_command(&board, action).await
        }
        Some(Commands::Rigs { action }) => {
            let board = Board::connect(&db_url()).await?;
            crate::commands::rigs::run_rigs_command(&board, action).await
        }
        Some(Commands::Skills { action }) => crate::skills::run_skills_command(action).await,
        Some(Commands::Logs { action }) => crate::logs::run_logs_command(action),
        Some(Commands::Run { task }) => {
            let rt = crate::runtime::init_runtime(cli.port).await?;
            if rt.worker.is_none() {
                anyhow::bail!("headless mode requires a worker; worker initialization failed");
            }
            let result = crate::headless::run_headless(&rt.board, &task).await;
            if let Some(ref worker) = rt.worker {
                worker.cancel();
            }
            result
        }
        None => {
            let log_rx = log_rx.expect("TUI mode must have log_rx");
            let rt = crate::runtime::init_runtime(cli.port).await?;
            let (agent, session_id) = crate::runtime::create_operator_agent().await?;
            let operator = Arc::new(opengoose_rig::rig::Operator::without_board(
                RigId::new("operator"),
                agent,
                &session_id,
            ));
            let result = crate::tui::run_tui(rt.board, operator, log_rx).await;
            if let Some(ref worker) = rt.worker {
                worker.cancel();
            }
            result
        }
    }
}

#[cfg(test)]
mod coverage_tests {
    use crate::cli::{BoardAction, RigsAction};
    use crate::commands::board::run_board_command;
    use crate::commands::rigs::run_rigs_command;
    use clap::Parser;
    use opengoose_board::Board;
    use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
    use std::sync::Arc;

    use crate::cli::{Cli, Commands};

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
        Arc::new(
            Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        )
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
            .expect("board operation should succeed");
        run_board_command(&board, BoardAction::Status)
            .await
            .expect("run_board_command should succeed");
    }

    #[tokio::test]
    async fn run_board_command_ready_and_create() {
        let board = new_board().await;
        run_board_command(&board, BoardAction::Ready)
            .await
            .expect("async operation should succeed");
        run_board_command(
            &board,
            BoardAction::Create {
                title: "new task".into(),
                priority: "P1".into(),
                tags: vec!["ui".into()],
            },
        )
        .await
        .expect("file read should succeed");
        let items = board.list().await.expect("list should succeed");
        assert_eq!(items.len(), 1);
        run_board_command(&board, BoardAction::Ready)
            .await
            .expect("async operation should succeed");
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
            .expect("board operation should succeed");

        run_board_command(&board, BoardAction::Claim { id: open.id })
            .await
            .expect("run_board_command should succeed");

        let claimed = board
            .get(open.id)
            .await
            .expect("get should succeed")
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
            .expect("board operation should succeed");
        run_board_command(&board, BoardAction::Submit { id: open.id })
            .await
            .expect("run_board_command should succeed");

        let done = board
            .get(open.id)
            .await
            .expect("get should succeed")
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
            .expect("board operation should succeed");
        run_board_command(&board, BoardAction::Abandon { id: abandon.id })
            .await
            .expect("run_board_command should succeed");

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
        .expect("board operation should succeed");
    }

    #[tokio::test]
    async fn run_rigs_command_cycle() {
        let board = new_board().await;
        run_rigs_command(&board, None)
            .await
            .expect("async operation should succeed");

        run_rigs_command(
            &board,
            Some(RigsAction::Add {
                id: "worker-01".into(),
                recipe: "small".into(),
                tags: vec!["tag".into()],
            }),
        )
        .await
        .expect("board operation should succeed");
        assert!(
            board
                .get_rig("worker-01")
                .await
                .expect("async operation should succeed")
                .is_some()
        );

        run_rigs_command(
            &board,
            Some(RigsAction::Trust {
                id: "worker-01".into(),
            }),
        )
        .await
        .expect("board operation should succeed");

        run_rigs_command(
            &board,
            Some(RigsAction::Remove {
                id: "worker-01".into(),
            }),
        )
        .await
        .expect("board operation should succeed");
        assert!(
            board
                .get_rig("worker-01")
                .await
                .expect("async operation should succeed")
                .is_none()
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]
    use crate::cli::BoardAction;
    use crate::cli::RigsAction;
    use crate::commands::board::{run_board_command, show_board};
    use crate::commands::rigs::run_rigs_command;
    use crate::headless::run_headless;
    use crate::runtime::{AgentConfig, create_agent};
    use crate::skills::test_env_lock;
    use opengoose_board::Board;
    use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
    use std::ffi::OsString;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn run_board_command_stamp_with_no_comment() {
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("async operation should succeed");
        let item = board
            .post(PostWorkItem {
                title: "stamp no comment".into(),
                description: String::new(),
                created_by: RigId::new("tester"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("board operation should succeed");

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
        .expect("board operation should succeed");
    }

    #[tokio::test]
    async fn run_board_command_create_with_invalid_priority_uses_default() {
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("async operation should succeed");
        run_board_command(
            &board,
            BoardAction::Create {
                title: "invalid priority task".into(),
                priority: "INVALID".into(),
                tags: vec![],
            },
        )
        .await
        .expect("board operation should succeed");
        let items = board.list().await.expect("list should succeed");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].priority, Priority::P1); // default
    }

    #[tokio::test]
    async fn run_board_command_covers_action_branches() {
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("async operation should succeed");
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
        .expect("board operation should succeed");

        let created = board
            .list()
            .await
            .expect("list should succeed")
            .into_iter()
            .find(|item| item.title == "task alpha")
            .expect("find should succeed")
            .id;

        run_board_command(&board, BoardAction::Status)
            .await
            .expect("run_board_command should succeed");
        run_board_command(&board, BoardAction::Ready)
            .await
            .expect("async operation should succeed");
        run_board_command(&board, BoardAction::Claim { id: created })
            .await
            .expect("run_board_command should succeed");
        run_board_command(&board, BoardAction::Submit { id: created })
            .await
            .expect("run_board_command should succeed");

        let stamp_target = board
            .post(PostWorkItem {
                title: "stamp target".into(),
                description: String::new(),
                created_by: RigId::new("tester"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("board operation should succeed")
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
        .expect("board operation should succeed");

        let abandon_target = board
            .post(PostWorkItem {
                title: "task beta".into(),
                description: String::new(),
                created_by: claimer.clone(),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .expect("board operation should succeed")
            .id;
        run_board_command(&board, BoardAction::Abandon { id: abandon_target })
            .await
            .expect("run_board_command should succeed");

        run_board_command(&board, BoardAction::Status)
            .await
            .expect("run_board_command should succeed");
    }

    #[tokio::test]
    async fn run_board_command_covers_empty_and_mixed_status_states() {
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("async operation should succeed");
        let claimer = RigId::new("mixed-claimer");
        let tester = RigId::new("tester");

        run_board_command(&board, BoardAction::Status)
            .await
            .expect("run_board_command should succeed");
        run_board_command(&board, BoardAction::Ready)
            .await
            .expect("async operation should succeed");

        let open_item = board
            .post(PostWorkItem {
                title: "open item".into(),
                description: String::new(),
                created_by: tester.clone(),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("file read should succeed");

        let claimed_source = board
            .post(PostWorkItem {
                title: "claimed item".into(),
                description: String::new(),
                created_by: tester.clone(),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .expect("file read should succeed");

        board
            .claim(open_item.id, &claimer)
            .await
            .expect("claim should succeed");
        board
            .claim(claimed_source.id, &claimer)
            .await
            .expect("claim should succeed");
        board
            .submit(open_item.id, &claimer)
            .await
            .expect("submit should succeed");

        run_board_command(&board, BoardAction::Status)
            .await
            .expect("run_board_command should succeed");
        run_board_command(&board, BoardAction::Ready)
            .await
            .expect("async operation should succeed");
    }

    #[tokio::test]
    async fn run_rigs_command_covers_add_list_remove_trust() {
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("async operation should succeed");

        run_rigs_command(
            &board,
            Some(RigsAction::Add {
                id: "r-test".into(),
                recipe: "echo recipe".into(),
                tags: vec!["fast".into()],
            }),
        )
        .await
        .expect("board operation should succeed");

        run_rigs_command(&board, None)
            .await
            .expect("async operation should succeed");
        run_rigs_command(
            &board,
            Some(RigsAction::Trust {
                id: "r-test".into(),
            }),
        )
        .await
        .expect("board operation should succeed");
        run_rigs_command(
            &board,
            Some(RigsAction::Remove {
                id: "r-test".into(),
            }),
        )
        .await
        .expect("board operation should succeed");
    }

    #[tokio::test]
    async fn run_rigs_command_covers_empty_and_list_branches() {
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("async operation should succeed");

        run_rigs_command(&board, None)
            .await
            .expect("async operation should succeed");

        run_rigs_command(
            &board,
            Some(RigsAction::Add {
                id: "r-empty".into(),
                recipe: "echo empty".into(),
                tags: vec![],
            }),
        )
        .await
        .expect("board operation should succeed");

        run_rigs_command(&board, None)
            .await
            .expect("async operation should succeed");

        run_rigs_command(
            &board,
            Some(RigsAction::Trust {
                id: "r-empty".into(),
            }),
        )
        .await
        .expect("board operation should succeed");

        run_rigs_command(
            &board,
            Some(RigsAction::Remove {
                id: "r-empty".into(),
            }),
        )
        .await
        .expect("board operation should succeed");
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
        let workdir = tempdir().expect("temp dir creation should succeed");
        let prev_home = set_env_var("HOME", workdir.path().to_str());
        let prev_provider = set_env_var("GOOSE_PROVIDER", Some("invalid-provider"));
        let prev_model = set_env_var("GOOSE_MODEL", None);

        let result = create_agent(AgentConfig {
            session_id: "opengoose".into(),
            system_prompt: None,
        })
        .await;
        assert!(
            result.is_err(),
            "create_agent should reject invalid provider"
        );

        restore_env_var("HOME", prev_home);
        restore_env_var("GOOSE_PROVIDER", prev_provider);
        restore_env_var("GOOSE_MODEL", prev_model);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn create_agent_rejects_invalid_model() {
        let _guard = test_env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let workdir = tempdir().expect("temp dir creation should succeed");
        let prev_home = set_env_var("HOME", workdir.path().to_str());
        let prev_provider = set_env_var("GOOSE_PROVIDER", Some("anthropic"));
        let prev_model = set_env_var("GOOSE_MODEL", Some("??invalid-model??"));

        let result = create_agent(AgentConfig {
            session_id: "worker".into(),
            system_prompt: None,
        })
        .await;
        assert!(result.is_err(), "create_agent should reject invalid model");

        restore_env_var("HOME", prev_home);
        restore_env_var("GOOSE_PROVIDER", prev_provider);
        restore_env_var("GOOSE_MODEL", prev_model);
    }

    #[tokio::test]
    async fn run_headless_times_out_when_no_worker_claims_task() {
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("async operation should succeed");
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            run_headless(&board, "solve test"),
        )
        .await;
        result.unwrap_err();

        let items = board.list().await.expect("list should succeed");
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn show_board_with_many_done_items_takes_5() {
        let board = Board::connect("sqlite::memory:")
            .await
            .expect("async operation should succeed");
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
                .expect("board operation should succeed");
            board
                .claim(item.id, &claimer)
                .await
                .expect("claim should succeed");
            board
                .submit(item.id, &claimer)
                .await
                .expect("submit should succeed");
        }
        show_board(&board)
            .await
            .expect("async operation should succeed");
    }

    #[tokio::test]
    async fn run_headless_completes_when_worker_submits() {
        let board = Arc::new(
            Board::connect("sqlite::memory:")
                .await
                .expect("async operation should succeed"),
        );
        let board2 = Arc::clone(&board);

        // Spawn a worker that submits the item once it's posted
        let worker = tokio::spawn(async move {
            // Give run_headless time to post the item
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            loop {
                let items = board2.list().await.expect("list should succeed");
                if let Some(item) = items.iter().find(|i| i.status == Status::Open) {
                    board2.claim(item.id, &RigId::new("worker")).await.ok(); // Test helper: best-effort claim to unblock run_headless
                    board2.submit(item.id, &RigId::new("worker")).await.ok(); // Test helper: best-effort submit to unblock run_headless
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
        let inner = result.expect("run_headless should complete within timeout");
        inner.expect("run_headless should return Ok");
        worker.await.expect("async operation should succeed");
    }

    #[tokio::test]
    async fn run_headless_bails_when_item_abandoned() {
        let board = Arc::new(
            Board::connect("sqlite::memory:")
                .await
                .expect("async operation should succeed"),
        );
        let board2 = Arc::clone(&board);

        let worker = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            loop {
                let items = board2.list().await.expect("list should succeed");
                if let Some(item) = items.iter().find(|i| i.status == Status::Open) {
                    board2.abandon(item.id).await.ok(); // Test helper: best-effort abandon to unblock run_headless
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
        let inner = result.expect("run_headless should complete within timeout");
        assert!(inner.is_err(), "should bail on abandoned");
        worker.await.expect("async operation should succeed");
    }
}
