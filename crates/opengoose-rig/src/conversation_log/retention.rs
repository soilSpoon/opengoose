// Log retention: listing, age-based cleanup, capacity-based cleanup.

use std::path::PathBuf;

use super::paths::log_dir;

/// Metadata for a session log file.
pub struct LogInfo {
    pub session_id: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: std::time::SystemTime,
}

/// List all log files (newest first).
pub fn list_logs() -> Vec<LogInfo> {
    let dir = log_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut logs: Vec<LogInfo> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            Some(LogInfo {
                session_id,
                path,
                size_bytes: meta.len(),
                modified: meta.modified().unwrap_or(std::time::UNIX_EPOCH),
            })
        })
        .collect();

    logs.sort_by(|a, b| b.modified.cmp(&a.modified));
    logs
}

/// Remove logs older than the given number of days. Returns count of removed files.
pub fn clean_older_than(days: u64) -> usize {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(days * 86400))
        .unwrap_or(std::time::UNIX_EPOCH);

    let logs = list_logs();
    logs.iter()
        .filter(|log| log.modified < cutoff)
        .filter(|log| std::fs::remove_file(&log.path).is_ok())
        .count()
}

/// Remove oldest logs when total size exceeds capacity. Returns count of removed files.
pub fn clean_over_capacity(max_bytes: u64) -> usize {
    let mut logs = list_logs();
    let total: u64 = logs.iter().map(|l| l.size_bytes).sum();
    if total <= max_bytes {
        return 0;
    }

    // Sort oldest first (ascending modification time)
    logs.sort_by(|a, b| a.modified.cmp(&b.modified));

    let mut current = total;
    let mut removed = 0;
    for log in &logs {
        if current <= max_bytes {
            break;
        }
        if std::fs::remove_file(&log.path).is_ok() {
            current -= log.size_bytes;
            removed += 1;
        }
    }
    removed
}
