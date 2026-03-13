use crate::db_enum::db_enum;
use crate::prolly::ProllyWorkItem;

db_enum! {
    /// Status of a work item.
    pub enum WorkStatus {
        Pending => "pending",
        InProgress => "in_progress",
        Completed => "completed",
        Failed => "failed",
        Cancelled => "cancelled",
        Compacted => "compacted",
    }
}

/// A tracked unit of work (inspired by Gas Town Beads / Goosetown issues).
#[derive(Debug, Clone)]
pub struct WorkItem {
    pub hash_id: String,
    pub session_key: String,
    pub team_run_id: String,
    pub parent_hash_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: WorkStatus,
    pub assigned_to: Option<String>,
    pub workflow_step: Option<i32>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub is_ephemeral: bool,
    pub priority: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl WorkItem {
    pub(crate) fn from_prolly(p: ProllyWorkItem) -> Self {
        Self {
            hash_id: p.hash_id,
            session_key: p.session_key,
            team_run_id: p.team_run_id,
            parent_hash_id: p.parent_hash_id,
            title: p.title,
            description: p.description,
            status: WorkStatus::parse(&p.status).unwrap_or(WorkStatus::Pending),
            assigned_to: p.assigned_to,
            workflow_step: p.workflow_step,
            input: p.input,
            output: p.output,
            error: p.error,
            is_ephemeral: p.is_ephemeral,
            priority: p.priority,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}
