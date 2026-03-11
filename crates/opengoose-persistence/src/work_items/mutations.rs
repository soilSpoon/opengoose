use diesel::prelude::*;
use tracing::debug;

use super::{WorkItemStore, WorkStatus};
use crate::db;
use crate::error::PersistenceResult;
use crate::models::NewWorkItem;
use crate::schema::work_items;

impl WorkItemStore {
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
}
