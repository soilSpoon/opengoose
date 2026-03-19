// Conversation Log — JSONL 기반 대화 이력 보존
//
// Goose 컴팩션 시 원본이 DELETE되므로, AgentEvent 스트림을
// 별도 JSONL 파일로 기록하여 원본 보존.
//
// 경로: ~/.opengoose/logs/{session-id}.jsonl

use chrono::Utc;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;

/// JSONL 로그 한 줄.
#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
}

/// 로그 디렉토리 경로.
pub fn log_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    home.join(".opengoose/logs")
}

/// 세션별 로그 파일 경로.
pub fn log_path(session_id: &str) -> PathBuf {
    log_dir().join(format!("{session_id}.jsonl"))
}

/// 로그 항목 추가 (append). 디렉토리가 없으면 생성.
pub fn append_entry(session_id: &str, role: &str, content: &str) {
    let dir = log_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }

    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
    };

    let path = log_path(session_id);
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };

    if let Ok(json) = serde_json::to_string(&entry) {
        let _ = writeln!(file, "{json}");
    }
}

/// 로그 디렉토리의 모든 세션 로그 정보.
pub struct LogInfo {
    pub session_id: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: std::time::SystemTime,
}

/// 모든 로그 파일 목록 (수정 시간 역순).
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

/// 보존 기간 초과 로그 삭제. 삭제된 파일 수 반환.
pub fn clean_older_than(days: u64) -> usize {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(days * 86400))
        .unwrap_or(std::time::UNIX_EPOCH);

    let logs = list_logs();
    let mut removed = 0;
    for log in &logs {
        if log.modified < cutoff {
            if std::fs::remove_file(&log.path).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

/// 최대 용량 초과 시 오래된 로그부터 삭제. 삭제된 파일 수 반환.
pub fn clean_over_capacity(max_bytes: u64) -> usize {
    let mut logs = list_logs();
    let total: u64 = logs.iter().map(|l| l.size_bytes).sum();
    if total <= max_bytes {
        return 0;
    }

    // 오래된 순으로 정렬 (수정 시간 오름차순)
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

/// 세션 로그의 전체 내용 읽기 (evolve.rs에서 사용).
pub fn read_log(session_id: &str) -> Option<String> {
    std::fs::read_to_string(log_path(session_id)).ok()
}

/// 세션 로그의 content만 추출하여 하나의 문자열로 결합.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_entry_roundtrip() {
        let entry = LogEntry {
            timestamp: "2026-03-19T10:00:00Z".into(),
            session_id: "test-session".into(),
            role: "user".into(),
            content: "hello".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: LogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_id, "test-session");
        assert_eq!(parsed.content, "hello");
    }

    #[test]
    fn append_and_read() {
        let tmp = tempfile::tempdir().unwrap();
        // Override log dir by writing directly
        let path = tmp.path().join("test-session.jsonl");
        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            session_id: "test-session".into(),
            role: "assistant".into(),
            content: "world".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        std::fs::write(&path, format!("{json}\n")).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: LogEntry = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(parsed.role, "assistant");
    }
}
