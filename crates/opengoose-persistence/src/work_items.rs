use std::sync::Arc;

use diesel::prelude::*;
use tracing::debug;

use crate::db::{self, Database};
use crate::db_enum::db_enum;
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{NewWorkItem, WorkItemRow};
use crate::schema::work_items;

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
    fn from_row(row: WorkItemRow) -> Result<Self, PersistenceError> {
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

/// Work item operations on a shared Database.
pub struct WorkItemStore {
    db: Arc<Database>,
}

impl WorkItemStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Create a new work item. Returns the auto-generated integer ID.
    pub fn create(
        &self,
        session_key: &str,
        team_run_id: &str,
        title: &str,
        parent_id: Option<i32>,
    ) -> PersistenceResult<i32> {
        self.db.with(|conn| {
            diesel::insert_into(work_items::table)
                .values(NewWorkItem {
                    session_key,
                    team_run_id,
                    parent_id,
                    title,
                })
                .execute(conn)?;
            // Retrieve the last inserted rowid (SQLite AUTOINCREMENT)
            let id = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>(
                "last_insert_rowid()",
            ))
            .get_result::<i32>(conn)?;
            debug!(id, title, "work item created");
            Ok(id)
        })
    }

    /// Update the status of a work item.
    pub fn update_status(&self, id: i32, status: WorkStatus) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(work_items::table.find(id))
                .set((
                    work_items::status.eq(status.as_str()),
                    work_items::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Assign a work item to an agent at a specific workflow step.
    pub fn assign(&self, id: i32, agent: &str, step: Option<i32>) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(work_items::table.find(id))
                .set((
                    work_items::assigned_to.eq(Some(agent)),
                    work_items::workflow_step.eq(step),
                    work_items::status.eq(WorkStatus::InProgress.as_str()),
                    work_items::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Set the input for a work item.
    pub fn set_input(&self, id: i32, input: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(work_items::table.find(id))
                .set((
                    work_items::input.eq(Some(input)),
                    work_items::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Set the output (result) for a work item and mark it completed.
    pub fn set_output(&self, id: i32, output: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(work_items::table.find(id))
                .set((
                    work_items::output.eq(Some(output)),
                    work_items::status.eq(WorkStatus::Completed.as_str()),
                    work_items::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Set the error message and mark the work item as failed.
    pub fn set_error(&self, id: i32, error: &str) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::update(work_items::table.find(id))
                .set((
                    work_items::error.eq(Some(error)),
                    work_items::status.eq(WorkStatus::Failed.as_str()),
                    work_items::updated_at.eq(db::now_sql()),
                ))
                .execute(conn)?;
            Ok(())
        })
    }

    /// Get a work item by ID.
    pub fn get(&self, id: i32) -> PersistenceResult<Option<WorkItem>> {
        self.db.with(|conn| {
            let result = work_items::table
                .find(id)
                .first::<WorkItemRow>(conn)
                .optional()?;
            match result {
                Some(row) => Ok(Some(WorkItem::from_row(row)?)),
                None => Ok(None),
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
            let rows = if let Some(status) = status {
                work_items::table
                    .filter(work_items::team_run_id.eq(team_run_id))
                    .filter(work_items::status.eq(status.as_str()))
                    .order((
                        work_items::workflow_step.asc(),
                        work_items::created_at.asc(),
                    ))
                    .load::<WorkItemRow>(conn)?
            } else {
                work_items::table
                    .filter(work_items::team_run_id.eq(team_run_id))
                    .order((
                        work_items::workflow_step.asc(),
                        work_items::created_at.asc(),
                    ))
                    .load::<WorkItemRow>(conn)?
            };
            rows.into_iter()
                .map(WorkItem::from_row)
                .collect::<Result<_, _>>()
        })
    }

    /// Get children of a parent work item.
    pub fn get_children(&self, parent_id: i32) -> PersistenceResult<Vec<WorkItem>> {
        self.db.with(|conn| {
            let rows = work_items::table
                .filter(work_items::parent_id.eq(parent_id))
                .order((
                    work_items::workflow_step.asc(),
                    work_items::created_at.asc(),
                ))
                .load::<WorkItemRow>(conn)?;
            rows.into_iter()
                .map(WorkItem::from_row)
                .collect::<Result<_, _>>()
        })
    }

    /// Find the resume point for a chain workflow: returns (next_step, last_output).
    pub fn find_resume_point(&self, parent_id: i32) -> PersistenceResult<Option<(i32, String)>> {
        self.db.with(|conn| {
            let result = work_items::table
                .filter(work_items::parent_id.eq(parent_id))
                .filter(work_items::status.eq(WorkStatus::Completed.as_str()))
                .order(work_items::workflow_step.desc())
                .select((work_items::workflow_step, work_items::output))
                .first::<(Option<i32>, Option<String>)>(conn)
                .optional()?;
            match result {
                Some((Some(step), output)) => Ok(Some((step + 1, output.unwrap_or_default()))),
                _ => Ok(None),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewSession;
    use crate::schema::sessions;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    fn ensure_session(db: &Arc<Database>, key: &str) {
        db.with(|conn| {
            diesel::insert_into(sessions::table)
                .values(NewSession { session_key: key })
                .on_conflict(sessions::session_key)
                .do_nothing()
                .execute(conn)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_create_and_get() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let id = store.create("sess1", "run1", "Fix auth bug", None).unwrap();
        assert!(id > 0);

        let item = store.get(id).unwrap().unwrap();
        assert_eq!(item.title, "Fix auth bug");
        assert_eq!(item.status, WorkStatus::Pending);
        assert!(item.parent_id.is_none());
    }

    #[test]
    fn test_assign_and_complete() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let id = store.create("sess1", "run1", "Step 1", None).unwrap();

        store.assign(id, "coder", Some(0)).unwrap();
        let item = store.get(id).unwrap().unwrap();
        assert_eq!(item.status, WorkStatus::InProgress);
        assert_eq!(item.assigned_to.as_deref(), Some("coder"));
        assert_eq!(item.workflow_step, Some(0));

        store.set_input(id, "input text").unwrap();
        store.set_output(id, "output text").unwrap();
        let item = store.get(id).unwrap().unwrap();
        assert_eq!(item.status, WorkStatus::Completed);
        assert_eq!(item.output.as_deref(), Some("output text"));
    }

    #[test]
    fn test_parent_children() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let parent_id = store.create("sess1", "run1", "Main task", None).unwrap();
        let child1 = store
            .create("sess1", "run1", "Step 0", Some(parent_id))
            .unwrap();
        let child2 = store
            .create("sess1", "run1", "Step 1", Some(parent_id))
            .unwrap();

        store.assign(child1, "coder", Some(0)).unwrap();
        store.assign(child2, "reviewer", Some(1)).unwrap();

        let children = store.get_children(parent_id).unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].workflow_step, Some(0));
        assert_eq!(children[1].workflow_step, Some(1));
    }

    #[test]
    fn test_find_resume_point() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let parent_id = store.create("sess1", "run1", "Chain task", None).unwrap();

        let step0 = store
            .create("sess1", "run1", "Step 0", Some(parent_id))
            .unwrap();
        let step1 = store
            .create("sess1", "run1", "Step 1", Some(parent_id))
            .unwrap();
        let _step2 = store
            .create("sess1", "run1", "Step 2", Some(parent_id))
            .unwrap();

        store.assign(step0, "coder", Some(0)).unwrap();
        store.set_output(step0, "step 0 output").unwrap();

        store.assign(step1, "reviewer", Some(1)).unwrap();
        store.set_error(step1, "timeout").unwrap();

        let point = store.find_resume_point(parent_id).unwrap();
        assert_eq!(point, Some((1, "step 0 output".to_string())));
    }

    #[test]
    fn test_list_for_run() {
        let db = test_db();
        ensure_session(&db, "sess1");
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

    #[test]
    fn test_work_status_as_str() {
        assert_eq!(WorkStatus::Pending.as_str(), "pending");
        assert_eq!(WorkStatus::InProgress.as_str(), "in_progress");
        assert_eq!(WorkStatus::Completed.as_str(), "completed");
        assert_eq!(WorkStatus::Failed.as_str(), "failed");
        assert_eq!(WorkStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_work_status_parse_roundtrip() {
        for s in [
            WorkStatus::Pending,
            WorkStatus::InProgress,
            WorkStatus::Completed,
            WorkStatus::Failed,
            WorkStatus::Cancelled,
        ] {
            assert_eq!(WorkStatus::parse(s.as_str()).unwrap(), s);
        }
    }

    #[test]
    fn test_work_status_parse_invalid() {
        let err = WorkStatus::parse("garbage").unwrap_err();
        assert!(err.to_string().contains("WorkStatus"));
    }

    #[test]
    fn test_get_nonexistent() {
        let db = test_db();
        let store = WorkItemStore::new(db);
        let result = store.get(99999).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_set_error() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let id = store.create("sess1", "run1", "Failing task", None).unwrap();
        store.set_error(id, "something went wrong").unwrap();

        let item = store.get(id).unwrap().unwrap();
        assert_eq!(item.status, WorkStatus::Failed);
        assert_eq!(item.error.as_deref(), Some("something went wrong"));
    }

    #[test]
    fn test_find_resume_point_no_children() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let parent = store.create("sess1", "run1", "Parent", None).unwrap();
        let result = store.find_resume_point(parent).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_for_run_filtered_by_status() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let id1 = store.create("sess1", "run1", "Task A", None).unwrap();
        store.create("sess1", "run1", "Task B", None).unwrap();
        store.set_output(id1, "done").unwrap();

        let completed = store
            .list_for_run("run1", Some(&WorkStatus::Completed))
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].title, "Task A");

        let pending = store
            .list_for_run("run1", Some(&WorkStatus::Pending))
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].title, "Task B");
    }

    #[test]
    fn test_update_status_cancelled() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let id = store.create("sess1", "run1", "Cancel me", None).unwrap();
        store.update_status(id, WorkStatus::Cancelled).unwrap();

        let item = store.get(id).unwrap().unwrap();
        assert_eq!(item.status, WorkStatus::Cancelled);
    }

    #[test]
    fn test_get_children_empty() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let parent_id = store.create("sess1", "run1", "Parent", None).unwrap();
        let children = store.get_children(parent_id).unwrap();
        assert!(children.is_empty());
    }

    #[test]
    fn test_find_resume_point_all_failed() {
        // When all children have failed (none completed), resume_point returns None.
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let parent_id = store.create("sess1", "run1", "Chain", None).unwrap();
        let step0 = store
            .create("sess1", "run1", "Step 0", Some(parent_id))
            .unwrap();
        store.assign(step0, "coder", Some(0)).unwrap();
        store.set_error(step0, "crashed").unwrap();

        let point = store.find_resume_point(parent_id).unwrap();
        assert!(point.is_none());
    }

    #[test]
    fn test_set_input() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let store = WorkItemStore::new(db);

        let id = store.create("sess1", "run1", "Process data", None).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().input, None);

        store.set_input(id, "raw payload").unwrap();
        let item = store.get(id).unwrap().unwrap();
        assert_eq!(item.input.as_deref(), Some("raw payload"));
        // Status should remain Pending (set_input does not change it).
        assert_eq!(item.status, WorkStatus::Pending);
    }
}
