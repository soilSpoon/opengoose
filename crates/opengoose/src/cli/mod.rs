// CLI argument definitions + logging setup

pub mod commands;
pub(crate) mod setup;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::tui;

/// Which run mode the CLI is entering — determines logging strategy.
pub enum RunMode {
    /// Interactive TUI: file + TuiLayer, no stderr
    Tui,
    /// Headless `run` subcommand: stderr + file
    Headless,
    /// Other CLI subcommands: stderr only
    CliSubcommand,
}

/// Set up tracing subscribers based on run mode.
/// Returns a `LogEntry` receiver when in TUI mode (needed for the Logs tab).
pub fn setup_logging(
    mode: RunMode,
) -> Result<Option<tokio::sync::mpsc::Receiver<tui::log_entry::LogEntry>>> {
    let env_filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "opengoose=info,goose=error".into())
    };

    match mode {
        RunMode::Tui => {
            let log_file = tui::log_entry::create_session_log_file()?;
            tui::log_entry::cleanup_old_logs(10)?;
            let (log_tx, log_rx) = tokio::sync::mpsc::channel::<tui::log_entry::LogEntry>(1000);
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer().with_writer(std::sync::Mutex::new(log_file)))
                .with(tui::tui_layer::TuiLayer::new(log_tx))
                .with(env_filter())
                .init();
            Ok(Some(log_rx))
        }
        RunMode::Headless => {
            let log_file = tui::log_entry::create_session_log_file()?;
            tui::log_entry::cleanup_old_logs(10)?;
            tracing_subscriber::registry()
                .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
                .with(tracing_subscriber::fmt::layer().with_writer(std::sync::Mutex::new(log_file)))
                .with(env_filter())
                .init();
            Ok(None)
        }
        RunMode::CliSubcommand => {
            tracing_subscriber::fmt()
                .with_env_filter(env_filter())
                .init();
            Ok(None)
        }
    }
}

#[derive(Parser)]
#[command(name = "opengoose", version = "0.2.0")]
#[command(about = "Goose-native pull architecture with Wasteland-level agent autonomy")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// 웹 대시보드 포트
    #[arg(long, default_value = "1355", global = true)]
    pub port: u16,
}

#[derive(Subcommand)]
pub enum Commands {
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
        action: crate::skills::SkillsAction,
    },
    /// 대화 로그 관리
    Logs {
        #[command(subcommand)]
        action: crate::logs::LogsAction,
    },
}

#[derive(Subcommand)]
pub enum BoardAction {
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
pub enum RigsAction {
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
