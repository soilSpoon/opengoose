//! Agent Rig — Goose 에이전트 실행 단위.
//! Operator(대화)와 Worker(작업 pull loop) 두 가지 모드로 동작한다.

use std::path::PathBuf;

pub mod agent_factory;
pub mod error;
pub use error::*;

pub mod conversation_log;
pub mod mcp_tools;
pub mod middleware;
pub mod pipeline;
pub mod rig;
pub mod work_mode;
pub mod worktree;

/// Return the user's home directory, preferring $HOME (for test isolation)
/// and falling back to `dirs::home_dir()`.
pub fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        PathBuf::from(h)
    } else {
        dirs::home_dir().unwrap_or_else(|| ".".into())
    }
}

#[cfg(test)]
pub(crate) mod test_fixtures;
