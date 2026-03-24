// JSONL log entry read/write + path helpers for conversation log storage.

use chrono::Utc;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use tracing::warn;

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

/// Shared env lock for tests that modify OPENGOOSE_HOME.
/// Must be a single static across all test modules in this crate.
#[cfg(test)]
pub(super) fn env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
pub(super) fn with_temp_home<F: FnOnce()>(home: &std::path::Path, action: F) {
    let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let previous_home = std::env::var_os("OPENGOOSE_HOME");
    unsafe {
        std::env::set_var("OPENGOOSE_HOME", home);
    }
    action();
    match previous_home {
        Some(v) => unsafe { std::env::set_var("OPENGOOSE_HOME", v) },
        None => unsafe { std::env::remove_var("OPENGOOSE_HOME") },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::env;
    use tempfile::tempdir;

    #[test]
    fn log_entry_serde_roundtrip_preserves_fields() {
        let entry = LogEntry {
            timestamp: "2026-03-19T10:00:00Z".into(),
            session_id: "test-session".into(),
            role: "user".into(),
            content: "hello".into(),
        };
        let json = serde_json::to_string(&entry).expect("LogEntry should serialize to JSON");
        let parsed: LogEntry =
            serde_json::from_str(&json).expect("LogEntry should deserialize from JSON");
        assert_eq!(parsed.session_id, "test-session");
        assert_eq!(parsed.content, "hello");
    }

    #[test]
    fn append_and_read() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        // Override log dir by writing directly
        let path = tmp.path().join("test-session.jsonl");
        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            session_id: "test-session".into(),
            role: "assistant".into(),
            content: "world".into(),
        };
        let json = serde_json::to_string(&entry).expect("JSON serialization should succeed");
        std::fs::write(&path, format!("{json}\n")).expect("test fixture write should succeed");

        let content = std::fs::read_to_string(&path).expect("test file read should succeed");
        let parsed: LogEntry = serde_json::from_str(content.trim()).expect("test JSON should parse");
        assert_eq!(parsed.role, "assistant");
    }

    #[test]
    fn read_log_returns_content_when_file_exists() {
        let tmp = tempdir().expect("temp dir creation should succeed");
        with_temp_home(tmp.path(), || {
            append_entry("test-read-session", "user", "hello world");
            let content = read_log("test-read-session");
            assert!(content.is_some());
            assert!(content.expect("content should be present").contains("hello world"));
        });
    }

    #[test]
    fn read_log_returns_none_when_file_missing() {
        let tmp = tempdir().expect("temp dir creation should succeed");
        with_temp_home(tmp.path(), || {
            let content = read_log("nonexistent-session");
            assert!(content.is_none());
        });
    }

    #[test]
    fn read_log_contents_parses_entries() {
        let tmp = tempdir().expect("temp dir creation should succeed");
        with_temp_home(tmp.path(), || {
            append_entry("parse-session", "user", "msg1");
            append_entry("parse-session", "assistant", "msg2");
            let entries = read_log_contents("parse-session");
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].role, "user");
            assert_eq!(entries[1].role, "assistant");
        });
    }

    #[test]
    fn read_log_contents_returns_empty_for_missing_file() {
        let tmp = tempdir().expect("temp dir creation should succeed");
        with_temp_home(tmp.path(), || {
            let entries = read_log_contents("no-such-session");
            assert!(entries.is_empty());
        });
    }

    #[test]
    fn log_dir_and_log_path_use_opengoose_home() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempdir().expect("temp dir creation should succeed");
        let prev = std::env::var_os("OPENGOOSE_HOME");
        unsafe {
            std::env::set_var("OPENGOOSE_HOME", tmp.path());
        }

        let dir = log_dir();
        assert!(dir.starts_with(tmp.path()));

        let path = log_path("test-session");
        assert!(path.to_string_lossy().contains("test-session"));

        unsafe {
            match prev {
                Some(v) => std::env::set_var("OPENGOOSE_HOME", v),
                None => std::env::remove_var("OPENGOOSE_HOME"),
            }
        }
    }

    #[test]
    fn opengoose_home_dir_falls_back_to_home_when_env_not_set() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempdir().expect("temp dir creation should succeed");
        let prev_home = std::env::var_os("HOME");
        let prev_og = std::env::var_os("OPENGOOSE_HOME");
        unsafe {
            std::env::remove_var("OPENGOOSE_HOME");
            std::env::set_var("HOME", tmp.path());
        }

        let dir = log_dir();
        // Without OPENGOOSE_HOME, should fall back to HOME-based path
        assert!(dir.starts_with(tmp.path()));

        unsafe {
            match prev_og {
                Some(v) => std::env::set_var("OPENGOOSE_HOME", v),
                None => std::env::remove_var("OPENGOOSE_HOME"),
            }
            match prev_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    /// Covers the early return when create_dir_all fails.
    /// Setting OPENGOOSE_HOME to a FILE path causes log_dir() to return a path whose
    /// ancestor component is a file -> create_dir_all returns ENOTDIR.
    #[test]
    fn append_entry_silently_ignores_create_dir_failure() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempdir().expect("temp dir creation should succeed");
        let prev = env::var_os("OPENGOOSE_HOME");

        // Create a FILE at the path we will use as OPENGOOSE_HOME
        let fake_home = tmp.path().join("notadir");
        std::fs::write(&fake_home, "file").expect("test fixture write should succeed");
        unsafe {
            env::set_var("OPENGOOSE_HOME", &fake_home);
        }

        // log_dir() returns <fake_home>/.opengoose/logs where <fake_home> is a FILE
        // create_dir_all fails with ENOTDIR -> early return
        append_entry("sid", "user", "msg"); // must not panic

        unsafe {
            match prev {
                Some(v) => env::set_var("OPENGOOSE_HOME", v),
                None => env::remove_var("OPENGOOSE_HOME"),
            }
        }
    }
}
