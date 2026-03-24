// Conversation Log — JSONL-based conversation history preservation
//
// Goose compaction DELETEs originals, so AgentEvent streams are
// recorded to separate JSONL files to preserve history.
//
// Path: ~/.opengoose/logs/{session-id}.jsonl

mod io;
mod retention;

pub use io::{LogEntry, append_entry, log_dir, log_path, read_log, read_log_contents};
pub use retention::{LogInfo, clean_older_than, clean_over_capacity, list_logs};
