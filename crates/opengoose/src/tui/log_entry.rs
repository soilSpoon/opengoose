use chrono::{DateTime, Utc};
use std::path::Path;
use tracing::Level;

/// TUI Logs 뷰에 표시되는 로그 항목.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: Level,
    pub target: String,
    pub message: String,
    /// 구조화된 이벤트인지 여부 (verbose 필터링용).
    pub structured: bool,
}

impl LogEntry {
    pub fn is_structured_target(target: &str) -> bool {
        target.starts_with("opengoose_rig::rig") || target.starts_with("opengoose::evolver")
    }
}

/// Creates `~/.opengoose/logs/opengoose-{timestamp}.log` and returns the file handle.
pub fn create_session_log_file() -> anyhow::Result<std::fs::File> {
    let log_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?
        .join(".opengoose")
        .join("logs");

    std::fs::create_dir_all(&log_dir)?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let filename = format!("opengoose-{timestamp}.log");
    let path = log_dir.join(filename);

    let file = std::fs::File::create(path)?;
    Ok(file)
}

/// Deletes oldest session log files under `~/.opengoose/logs`, keeping only `keep` most recent.
pub fn cleanup_old_logs(keep: usize) -> anyhow::Result<()> {
    let log_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?
        .join(".opengoose")
        .join("logs");

    cleanup_old_logs_in(&log_dir, keep)
}

/// Internal, testable version — takes the directory as a parameter.
pub(crate) fn cleanup_old_logs_in(log_dir: &Path, keep: usize) -> anyhow::Result<()> {
    if !log_dir.exists() {
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(log_dir)?
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "log")
                .unwrap_or(false)
        })
        .collect();

    // Sort by modification time, oldest first.
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

    let excess = entries.len().saturating_sub(keep);
    for entry in entries.into_iter().take(excess) {
        std::fs::remove_file(entry.path())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    static LOG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn is_structured_target_matches_rig_and_evolver() {
        assert!(LogEntry::is_structured_target("opengoose_rig::rig"));
        assert!(LogEntry::is_structured_target(
            "opengoose_rig::rig::something"
        ));
        assert!(LogEntry::is_structured_target("opengoose::evolver"));
        assert!(!LogEntry::is_structured_target("goose::agents"));
        assert!(!LogEntry::is_structured_target("opengoose::web"));
    }

    #[test]
    fn cleanup_old_logs_removes_excess() {
        let dir = tempfile::tempdir().expect("temp dir creation should succeed");
        let log_dir = dir.path();

        // Create 5 log files
        for i in 0..5 {
            let path = log_dir.join(format!("opengoose-test-{i}.log"));
            std::fs::write(&path, "test").expect("test fixture write should succeed");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        cleanup_old_logs_in(log_dir, 3).expect("cleanup should succeed");

        let remaining: Vec<_> = std::fs::read_dir(log_dir)
            .expect("directory read should succeed")
            .flatten()
            .collect();
        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn cleanup_old_logs_in_returns_ok_when_dir_does_not_exist() {
        let non_existent = std::path::Path::new("/tmp/opengoose_test_nonexistent_dir_xyz_12345");
        let _ = std::fs::remove_dir_all(non_existent);
        assert!(!non_existent.exists());
        let result = cleanup_old_logs_in(non_existent, 5);
        assert!(result.is_ok());
    }

    #[test]
    fn cleanup_old_logs_noop_when_under_limit() {
        let dir = tempfile::tempdir().expect("temp dir creation should succeed");
        let log_dir = dir.path();

        for i in 0..2 {
            let path = log_dir.join(format!("opengoose-test-{i}.log"));
            std::fs::write(&path, "test").expect("test fixture write should succeed");
        }

        cleanup_old_logs_in(log_dir, 10).expect("cleanup should succeed");

        let remaining: Vec<_> = std::fs::read_dir(log_dir)
            .expect("directory read should succeed")
            .flatten()
            .collect();
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn cleanup_old_logs_in_skips_non_log_files() {
        let dir = tempfile::tempdir().expect("temp dir creation should succeed");
        let log_dir = dir.path();

        // Create 3 log files and 2 non-log files
        for i in 0..3 {
            std::fs::write(log_dir.join(format!("entry-{i}.log")), "log")
                .expect("test fixture write should succeed");
        }
        std::fs::write(log_dir.join("README.txt"), "skip")
            .expect("test fixture write should succeed");
        std::fs::write(log_dir.join("data.json"), "skip")
            .expect("test fixture write should succeed");

        // keep=1, only .log files are candidates → 2 removed, non-logs untouched
        cleanup_old_logs_in(log_dir, 1).expect("cleanup should succeed");

        let all: Vec<_> = std::fs::read_dir(log_dir)
            .expect("directory read should succeed")
            .flatten()
            .collect();
        let log_count = all
            .iter()
            .filter(|e| e.path().extension().map(|x| x == "log").unwrap_or(false))
            .count();
        let non_log_count = all
            .iter()
            .filter(|e| e.path().extension().map(|x| x != "log").unwrap_or(true))
            .count();

        assert_eq!(log_count, 1);
        assert_eq!(non_log_count, 2); // README.txt and data.json untouched
    }

    #[test]
    fn log_entry_fields_are_accessible() {
        let ts = Utc::now();
        let entry = LogEntry {
            timestamp: ts,
            level: Level::INFO,
            target: "opengoose::test".to_string(),
            message: "hello".to_string(),
            structured: false,
        };
        assert_eq!(entry.target, "opengoose::test");
        assert_eq!(entry.message, "hello");
        assert!(!entry.structured);
        assert_eq!(entry.level, Level::INFO);
    }

    #[test]
    fn log_entry_clone_preserves_fields() {
        let entry = LogEntry {
            timestamp: Utc::now(),
            level: Level::WARN,
            target: "t".to_string(),
            message: "m".to_string(),
            structured: true,
        };
        let cloned = entry.clone();
        assert_eq!(cloned.message, "m");
        assert_eq!(cloned.level, Level::WARN);
        assert!(cloned.structured);
    }

    #[test]
    fn create_session_log_file_creates_file_in_home() {
        let guard = LOG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let prev = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", tmp.path());
        }

        let file = create_session_log_file();

        match prev {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        assert!(
            file.is_ok(),
            "should create session log file: {:?}",
            file.err()
        );
        drop(guard);
    }

    #[test]
    fn cleanup_old_logs_runs_against_temp_home() {
        let guard = LOG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let prev = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", tmp.path());
        }

        let result = cleanup_old_logs(100);

        match prev {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
        assert!(result.is_ok());
        drop(guard);
    }
}
