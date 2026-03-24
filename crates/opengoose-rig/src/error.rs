use opengoose_board::BoardError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RigError {
    #[error("session failed: {0}")]
    SessionFailed(String),
    #[error("worktree failed: {0}")]
    WorktreeFailed(String),
    #[error("board error: {0}")]
    Board(#[from] BoardError),
    #[error("middleware error: {0}")]
    Middleware(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
