use std::sync::Arc;

use rusqlite::params;
use tracing::debug;
use uuid::Uuid;

use crate::db::Database;
use crate::error::{PersistenceError, PersistenceResult};

/// Status of a work item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl WorkStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, PersistenceError> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(PersistenceError::InvalidEnumValue(format!(
                "unknown WorkStatus: {other}"
            ))),
        }
    }
}

/// A tracked unit of work (inspired by Gas Town Beads / Goosetown issues).
#[derive(Debug, Clone)]
pub struct WorkItem {
    pub id: String,
    pub session_key: String,
    pub team_run_id: String,
    pub parent_id: Option<String>,
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

/// Work item operations on a shared Database.
pub struct WorkItemStore {
    db: Arc<Database>,
}

impl WorkItemStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Generate a short unique ID for a work item (e.g. "wi-a3f8e1").
    fn generate_id() -> String {
        let uuid = Uuid::new_v4();
        let hex = format!("{uuid:x}");
        format!("wi-{}", &hex[..12])
    }

    /// Create a new work item.
    pub fn create(
        &self,
        session_key: &str,
        team_run_id: &str,
        title: &str,
        parent_id: Option<&str>,
    ) -> PersistenceResult<String> {
        let id = Self::generate_id();
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO work_items (id, session_key, team_run_id, parent_id, title)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, session_key, team_run_id, parent_id, title],
            )?;
            debug!(id = %id, title, "work item created");
            Ok(id)
        })
    }

    /// Update the status of a work item.
    pub fn update_status(&self, id: &str, status: WorkStatus) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE work_items SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![status.as_str(), id],
            )?;
            Ok(())
        })
    }

    /// Assign a work item to an agent at a specific workflow step.
    pub fn assign(
        &self,
        id: &str,
        agent: &str,
        step: Option<i32>,
    ) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE work_items SET assigned_to = ?1, workflow_step = ?2, status = 'in_progress', updated_at = datetime('now') WHERE id = ?3",
                params![agent, step, id],
            )?;
            Ok(())
        })
    }

    /// Set the input for a work item.
    pub fn set_input(&self, id: &str, input: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE work_items SET input = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![input, id],
            )?;
            Ok(())
        })
    }

    /// Set the output (result) for a work item and mark it completed.
    pub fn set_output(&self, id: &str, output: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE work_items SET output = ?1, status = 'completed', updated_at = datetime('now') WHERE id = ?2",
                params![output, id],
            )?;
            Ok(())
        })
    }

    /// Set the error message and mark the work item as failed.
    pub fn set_error(&self, id: &str, error: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE work_items SET error = ?1, status = 'failed', updated_at = datetime('now') WHERE id = ?2",
                params![error, id],
            )?;
            Ok(())
        })
    }

    /// Get a work item by ID.
    pub fn get(&self, id: &str) -> PersistenceResult<Option<WorkItem>> {
        self.db.with(|conn| {
            let result = conn.query_row(
                "SELECT id, session_key, team_run_id, parent_id, title, description, status,
                        assigned_to, workflow_step, input, output, error, created_at, updated_at
                 FROM work_items WHERE id = ?1",
                params![id],
                Self::row_to_work_item,
            );
            match result {
                Ok(item) => Ok(Some(item)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    /// List work items for a team run, optionally filtered by status.
    pub fn list_for_run(
        &self,
        team_run_id: &str,
        status: Option<&WorkStatus>,
    ) -> PersistenceResult<Vec<WorkItem>> {
        self.db.with(|conn| {
            let items = if let Some(status) = status {
                let mut stmt = conn.prepare(
                    "SELECT id, session_key, team_run_id, parent_id, title, description, status,
                            assigned_to, workflow_step, input, output, error, created_at, updated_at
                     FROM work_items WHERE team_run_id = ?1 AND status = ?2
                     ORDER BY workflow_step ASC, created_at ASC",
                )?;
                stmt.query_map(params![team_run_id, status.as_str()], Self::row_to_work_item)?
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, session_key, team_run_id, parent_id, title, description, status,
                            assigned_to, workflow_step, input, output, error, created_at, updated_at
                     FROM work_items WHERE team_run_id = ?1
                     ORDER BY workflow_step ASC, created_at ASC",
                )?;
                stmt.query_map(params![team_run_id], Self::row_to_work_item)?
                    .collect::<Result<Vec<_>, _>>()?
            };
            Ok(items)
        })
    }

    /// Get children of a parent work item.
    pub fn get_children(&self, parent_id: &str) -> PersistenceResult<Vec<WorkItem>> {
        self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_key, team_run_id, parent_id, title, description, status,
                        assigned_to, workflow_step, input, output, error, created_at, updated_at
                 FROM work_items WHERE parent_id = ?1
                 ORDER BY workflow_step ASC, created_at ASC",
            )?;
            let items: Vec<WorkItem> = stmt
                .query_map(params![parent_id], Self::row_to_work_item)?
                .collect::<Result<_, _>>()?;
            Ok(items)
        })
    }

    /// Find the resume point for a chain workflow: returns (next_step, last_output).
    /// Looks for the highest completed step and returns step + 1 with its output.
    pub fn find_resume_point(&self, parent_id: &str) -> PersistenceResult<Option<(i32, String)>> {
        self.db.with(|conn| {
            let result = conn.query_row(
                "SELECT workflow_step, output FROM work_items
                 WHERE parent_id = ?1 AND status = 'completed'
                 ORDER BY workflow_step DESC
                 LIMIT 1",
                params![parent_id],
                |row| {
                    let step: i32 = row.get(0)?;
                    let output: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                    Ok((step + 1, output))
                },
            );
            match result {
                Ok(point) => Ok(Some(point)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    fn row_to_work_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkItem> {
        Ok(WorkItem {
            id: row.get(0)?,
            session_key: row.get(1)?,
            team_run_id: row.get(2)?,
            parent_id: row.get(3)?,
            title: row.get(4)?,
            description: row.get(5)?,
            status: WorkStatus::from_str(&row.get::<_, String>(6)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            assigned_to: row.get(7)?,
            workflow_step: row.get(8)?,
            input: row.get(9)?,
            output: row.get(10)?,
            error: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn test_create_and_get() {
        let db = test_db();
        let store = WorkItemStore::new(db);

        let id = store.create("sess1", "run1", "Fix auth bug", None).unwrap();
        assert!(id.starts_with("wi-"));

        let item = store.get(&id).unwrap().unwrap();
        assert_eq!(item.title, "Fix auth bug");
        assert_eq!(item.status, WorkStatus::Pending);
        assert!(item.parent_id.is_none());
    }

    #[test]
    fn test_assign_and_complete() {
        let db = test_db();
        let store = WorkItemStore::new(db);

        let id = store.create("sess1", "run1", "Step 1", None).unwrap();

        store.assign(&id, "coder", Some(0)).unwrap();
        let item = store.get(&id).unwrap().unwrap();
        assert_eq!(item.status, WorkStatus::InProgress);
        assert_eq!(item.assigned_to.as_deref(), Some("coder"));
        assert_eq!(item.workflow_step, Some(0));

        store.set_input(&id, "input text").unwrap();
        store.set_output(&id, "output text").unwrap();
        let item = store.get(&id).unwrap().unwrap();
        assert_eq!(item.status, WorkStatus::Completed);
        assert_eq!(item.output.as_deref(), Some("output text"));
    }

    #[test]
    fn test_parent_children() {
        let db = test_db();
        let store = WorkItemStore::new(db);

        let parent_id = store.create("sess1", "run1", "Main task", None).unwrap();
        let child1 = store
            .create("sess1", "run1", "Step 0", Some(&parent_id))
            .unwrap();
        let child2 = store
            .create("sess1", "run1", "Step 1", Some(&parent_id))
            .unwrap();

        store.assign(&child1, "coder", Some(0)).unwrap();
        store.assign(&child2, "reviewer", Some(1)).unwrap();

        let children = store.get_children(&parent_id).unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].workflow_step, Some(0));
        assert_eq!(children[1].workflow_step, Some(1));
    }

    #[test]
    fn test_find_resume_point() {
        let db = test_db();
        let store = WorkItemStore::new(db);

        let parent_id = store.create("sess1", "run1", "Chain task", None).unwrap();

        let step0 = store
            .create("sess1", "run1", "Step 0", Some(&parent_id))
            .unwrap();
        let step1 = store
            .create("sess1", "run1", "Step 1", Some(&parent_id))
            .unwrap();
        let _step2 = store
            .create("sess1", "run1", "Step 2", Some(&parent_id))
            .unwrap();

        store.assign(&step0, "coder", Some(0)).unwrap();
        store.set_output(&step0, "step 0 output").unwrap();

        store.assign(&step1, "reviewer", Some(1)).unwrap();
        store.set_error(&step1, "timeout").unwrap();

        // Resume point should be step 1 (last completed was 0)
        let point = store.find_resume_point(&parent_id).unwrap();
        assert_eq!(point, Some((1, "step 0 output".to_string())));
    }

    #[test]
    fn test_list_for_run() {
        let db = test_db();
        let store = WorkItemStore::new(db);

        store.create("sess1", "run1", "Task A", None).unwrap();
        store.create("sess1", "run1", "Task B", None).unwrap();
        store.create("sess1", "run2", "Task C", None).unwrap();

        let items = store.list_for_run("run1", None).unwrap();
        assert_eq!(items.len(), 2);

        let items = store
            .list_for_run("run1", Some(&WorkStatus::Pending))
            .unwrap();
        assert_eq!(items.len(), 2);
    }
}
