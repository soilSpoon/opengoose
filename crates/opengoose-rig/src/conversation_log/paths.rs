// Path helpers for conversation log storage.

use std::path::PathBuf;

pub(crate) fn opengoose_home_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("OPENGOOSE_HOME") {
        PathBuf::from(home)
    } else {
        dirs::home_dir().unwrap_or_else(|| ".".into())
    }
}

/// Log directory path.
pub fn log_dir() -> PathBuf {
    let home = opengoose_home_dir();
    home.join(".opengoose/logs")
}

/// Per-session log file path.
pub fn log_path(session_id: &str) -> PathBuf {
    log_dir().join(format!("{session_id}.jsonl"))
}
