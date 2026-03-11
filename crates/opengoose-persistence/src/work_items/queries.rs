use diesel::prelude::*;

use super::{WorkItem, WorkItemStore, WorkStatus};
use crate::error::PersistenceResult;
use crate::models::WorkItemRow;
use crate::schema::work_items;

impl WorkItemStore {
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
