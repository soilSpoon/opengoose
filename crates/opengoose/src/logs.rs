// Logs CLI — 대화 로그 관리
//
// opengoose logs list              — 세션별 로그 목록
// opengoose logs clean             — 보존 기간(30일) 초과 로그 삭제
// opengoose logs clean --older-than 7d — 7일 이상 로그 삭제

use clap::Subcommand;
use opengoose_rig::conversation_log;

#[derive(Subcommand)]
pub enum LogsAction {
    /// 세션별 로그 목록
    List,
    /// 보존 기간 초과 로그 삭제
    Clean {
        /// 삭제 기준 기간 (예: 7d, 30d). 기본: 30d
        #[arg(long, default_value = "30d")]
        older_than: String,
    },
}

pub fn run_logs_command(action: LogsAction) -> anyhow::Result<()> {
    match action {
        LogsAction::List => {
            let logs = conversation_log::list_logs();
            if logs.is_empty() {
                println!("No conversation logs found.");
                return Ok(());
            }

            println!("{:<40} {:>10}  {}", "Session", "Size", "Modified");
            println!("{}", "-".repeat(70));
            for log in &logs {
                let size = format_bytes(log.size_bytes);
                let modified = log
                    .modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| {
                        let dt = chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                            .unwrap_or_default();
                        dt.format("%Y-%m-%d %H:%M").to_string()
                    })
                    .unwrap_or_else(|_| "unknown".into());
                println!("{:<40} {:>10}  {}", log.session_id, size, modified);
            }

            let total: u64 = logs.iter().map(|l| l.size_bytes).sum();
            println!("\n{} logs, {} total", logs.len(), format_bytes(total));
        }
        LogsAction::Clean { older_than } => {
            let days = parse_duration_days(&older_than)?;
            let removed = conversation_log::clean_older_than(days);
            println!("Removed {removed} log(s) older than {days} days.");

            // 용량 체크 (100MB)
            let cap_removed = conversation_log::clean_over_capacity(100 * 1024 * 1024);
            if cap_removed > 0 {
                println!("Removed {cap_removed} additional log(s) over 100MB capacity.");
            }
        }
    }
    Ok(())
}

fn parse_duration_days(s: &str) -> anyhow::Result<u64> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix('d') {
        num.parse::<u64>()
            .map_err(|_| anyhow::anyhow!("invalid duration: {s}"))
    } else if let Ok(num) = s.parse::<u64>() {
        Ok(num)
    } else {
        Err(anyhow::anyhow!(
            "invalid duration format: {s} (expected e.g. 7d or 30d)"
        ))
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
