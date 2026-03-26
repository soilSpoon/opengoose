use opengoose_board::BoardError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RigError {
    #[error("worktree failed: {0}")]
    WorktreeFailed(String),
    #[error("board error: {0}")]
    Board(#[from] BoardError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
