// JSONL log entry read/write operations.

use chrono::Utc;
use serde::Serialize;
use std::io::Write;
use tracing::warn;

use super::paths::{log_dir, log_path};

/// JSONL log entry (one line).
#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
}

// LogEntry needs Deserialize for read_log_contents
impl<'de> serde::Deserialize<'de> for LogEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Inner {
            timestamp: String,
            session_id: String,
            role: String,
            content: String,
        }
        let inner = Inner::deserialize(deserializer)?;
        Ok(LogEntry {
            timestamp: inner.timestamp,
            session_id: inner.session_id,
            role: inner.role,
            content: inner.content,
        })
    }
}

/// Append a log entry. Creates directory if needed.
pub fn append_entry(session_id: &str, role: &str, content: &str) {
    let dir = log_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!(session_id, error = %e, "failed to create log directory");
        return;
    }

    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
    };

    let path = log_path(session_id);
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            warn!(session_id, path = %path.display(), error = %e, "failed to open log file");
            return;
        }
    };

    match serde_json::to_string(&entry) {
        Ok(json) => {
            if let Err(e) = writeln!(file, "{json}") {
                warn!(session_id, error = %e, "failed to write log entry");
            }
        }
        Err(e) => {
            warn!(session_id, error = %e, "failed to serialize log entry");
        }
    }
}

/// Read full session log contents as a string.
pub fn read_log(session_id: &str) -> Option<String> {
    std::fs::read_to_string(log_path(session_id)).ok()
}

/// Parse session log into individual entries.
pub fn read_log_contents(session_id: &str) -> Vec<LogEntry> {
    let content = match std::fs::read_to_string(log_path(session_id)) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content
        .lines()
        .filter_map(|line| serde_json::from_str::<LogEntry>(line).ok())
        .collect()
}
