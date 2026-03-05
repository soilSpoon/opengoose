use crate::db_enum::db_enum;

db_enum! {
    /// Status of an orchestration or workflow run.
    pub enum RunStatus {
        Running => "running",
        Completed => "completed",
        Failed => "failed",
        Suspended => "suspended",
    }
}
