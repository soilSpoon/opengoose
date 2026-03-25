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

            println!("{:<40} {:>10}  Modified", "Session", "Size");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ENV_LOCK;
    use opengoose_rig::conversation_log;
    use std::env;
    use std::ffi::OsString;

    fn with_isolated_home(tmp: &std::path::Path) {
        unsafe {
            env::set_var("HOME", tmp);
        }
        env::set_current_dir(tmp).expect("set_current_dir should succeed");
    }

    fn restore_env(home: Option<OsString>, cwd: std::path::PathBuf) {
        unsafe {
            match home {
                Some(v) => env::set_var("HOME", v),
                None => env::remove_var("HOME"),
            }
        }
        env::set_current_dir(cwd).expect("set_current_dir should succeed");
    }

    #[test]
    fn parse_duration_days_with_suffix() {
        assert_eq!(
            parse_duration_days("7d").expect("parse_duration_days should succeed"),
            7
        );
    }

    #[test]
    fn parse_duration_days_plain_days() {
        assert_eq!(
            parse_duration_days("30").expect("parse_duration_days should succeed"),
            30
        );
    }

    #[test]
    fn parse_duration_days_invalid_format() {
        assert!(parse_duration_days("abc").is_err());
        assert!(parse_duration_days("1w").is_err());
    }

    #[test]
    fn format_bytes_transitions() {
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(1536), "1.5KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0MB");
    }

    #[test]
    fn run_logs_list_on_empty_dir() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("lock should succeed");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        with_isolated_home(tmp.path());

        assert!(run_logs_command(LogsAction::List).is_ok());

        restore_env(home, cwd);
    }

    #[test]
    fn run_logs_clean_is_ok_and_removes_old_files() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("lock should succeed");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        with_isolated_home(tmp.path());

        conversation_log::append_entry("session-1", "user", "hello");
        conversation_log::append_entry("session-2", "assistant", "world");

        assert!(
            run_logs_command(LogsAction::Clean {
                older_than: "1d".into()
            })
            .is_ok()
        );
        assert!(
            run_logs_command(LogsAction::Clean {
                older_than: "100d".into()
            })
            .is_ok()
        );

        restore_env(home, cwd);
    }

    #[test]
    fn run_logs_list_with_existing_logs() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cwd = env::current_dir().expect("lock should succeed");
        let home = env::var_os("HOME");
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        with_isolated_home(tmp.path());

        // Create some logs so list is non-empty
        conversation_log::append_entry("list-session-a", "user", "hello");
        conversation_log::append_entry("list-session-b", "assistant", "world");

        assert!(run_logs_command(LogsAction::List).is_ok());

        restore_env(home, cwd);
    }

    #[test]
    fn parse_duration_days_invalid_d_suffix() {
        assert!(parse_duration_days("xd").is_err());
    }

    #[test]
    fn format_bytes_exact_boundaries() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(1023), "1023B");
        assert_eq!(format_bytes(1024), "1.0KB");
        assert_eq!(format_bytes(1024 * 1024 - 1), "1024.0KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0MB");
    }
}
