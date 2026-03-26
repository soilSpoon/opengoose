// opengoose-rig — Agent Rig (영속 pull 루프)
//
// Goose Agent::reply()를 감싸는 최소 래퍼.
// 메시지 라우팅, 플랫폼 관리, 데이터 저장은 하지 않는다.

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
