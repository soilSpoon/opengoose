use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("skill load failed: {0}")]
    LoadFailed(String),
    #[error("invalid frontmatter: {0}")]
    InvalidFrontmatter(String),
    #[error("evolution failed: {0}")]
    EvolutionFailed(String),
    #[error("skill not found: {0}")]
    NotFound(String),
    #[error("filesystem error: {0}")]
    Fs(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
