//! Work item persistence for orchestration and team execution flows.
//!
//! Provides [`WorkItemStore`] for creating, updating, and querying tracked
//! units of work while keeping the value types in a dedicated module.

#[cfg(test)]
mod tests;
mod types;

use std::sync::Arc;

use diesel::prelude::*;
use tracing::debug;

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::{NewWorkItem, WorkItemRow};
use crate::schema::work_items;

pub use types::{WorkItem, WorkStatus};

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
            let row = diesel::insert_into(work_items::table)
                .values(NewWorkItem {
                    session_key,
                    team_run_id,
                    parent_id,
                    title,
                })
                .get_result::<WorkItemRow>(conn)?;
            debug!(id = row.id, title, "work item created");
            Ok(row.id)
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
