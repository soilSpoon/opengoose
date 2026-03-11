use crate::db_enum::db_enum;
use crate::error::PersistenceError;
use crate::models::WorkItemRow;

db_enum! {
    /// Status of a work item.
    pub enum WorkStatus {
        Pending => "pending",
        InProgress => "in_progress",
        Completed => "completed",
        Failed => "failed",
        Cancelled => "cancelled",
    }
}

/// A tracked unit of work (inspired by Gas Town Beads / Goosetown issues).
#[derive(Debug, Clone)]
pub struct WorkItem {
    pub id: i32,
    pub session_key: String,
    pub team_run_id: String,
    pub parent_id: Option<i32>,
    pub title: String,
    pub description: Option<String>,
    pub status: WorkStatus,
    pub assigned_to: Option<String>,
    pub workflow_step: Option<i32>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl WorkItem {
    pub(crate) fn from_row(row: WorkItemRow) -> Result<Self, PersistenceError> {
        Ok(Self {
            status: WorkStatus::parse(&row.status)?,
            id: row.id,
            session_key: row.session_key,
            team_run_id: row.team_run_id,
            parent_id: row.parent_id,
            title: row.title,
            description: row.description,
            assigned_to: row.assigned_to,
            workflow_step: row.workflow_step,
            input: row.input,
            output: row.output,
            error: row.error,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}
