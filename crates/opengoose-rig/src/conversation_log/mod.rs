// Conversation Log — JSONL-based conversation history preservation
//
// Goose compaction DELETEs originals, so AgentEvent streams are
// recorded to separate JSONL files to preserve history.
//
// Path: ~/.opengoose/logs/{session-id}.jsonl

mod io;
mod retention;

pub use io::{append_entry, log_dir, log_path, read_log, read_log_contents, LogEntry};
pub use retention::{clean_older_than, clean_over_capacity, list_logs, LogInfo};
