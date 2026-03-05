use crate::error::PersistenceError;

/// Status of an orchestration or workflow run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Suspended,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Suspended => "suspended",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, PersistenceError> {
        match s {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "suspended" => Ok(Self::Suspended),
            other => Err(PersistenceError::InvalidEnumValue(format!(
                "unknown RunStatus: {other}"
            ))),
        }
    }
}
