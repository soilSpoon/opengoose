use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("hypervisor error: {0} (code {1})")]
    Hypervisor(String, i32),

    #[error("boot failed: {0}")]
    Boot(String),

    #[error("snapshot error: {0}")]
    Snapshot(String),

    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SandboxError>;
