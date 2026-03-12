use crate::error::PersistenceError;
use crate::models::OrchestrationRunRow;
use crate::run_status::RunStatus;

/// A tracked orchestration run (for crash recovery).
#[derive(Debug, Clone)]
pub struct OrchestrationRun {
    pub team_run_id: String,
    pub session_key: String,
    pub team_name: String,
    pub workflow: String,
    pub input: String,
    pub status: RunStatus,
    pub current_step: i32,
    pub total_steps: i32,
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl OrchestrationRun {
    pub(crate) fn from_row(row: OrchestrationRunRow) -> Result<Self, PersistenceError> {
        Ok(Self {
            status: RunStatus::parse(&row.status)?,
            team_run_id: row.team_run_id,
            session_key: row.session_key,
            team_name: row.team_name,
            workflow: row.workflow,
            input: row.input,
            current_step: row.current_step,
            total_steps: row.total_steps,
            result: row.result,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}
