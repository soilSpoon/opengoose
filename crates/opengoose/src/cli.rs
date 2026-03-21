// CLI argument definitions — Clap structs for opengoose

use clap::{Parser, Subcommand};

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
