//! The `compact()` algorithm: summarizes old completed tasks.
//!
//! Groups completed items by parent, stores a digest, and marks originals
//! with status = 'compacted'.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use diesel::prelude::*;

use crate::db::{self, Database};
use crate::error::PersistenceResult;
use crate::models::NewCompacted;
use crate::schema::{work_item_compacted, work_items};
use crate::work_items::WorkStatus;

/// Store for compact() operations.
pub struct CompactStore {
    db: Arc<Database>,
}

impl CompactStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Compact completed work items older than `older_than` duration.
    ///
    /// Groups items by parent_id, creates a summary digest for each group,
    /// and marks the originals as 'compacted'.
    ///
    /// Returns the number of items compacted.
    pub fn compact(&self, team_run_id: &str, older_than: Duration) -> PersistenceResult<usize> {
        self.db.with(|conn| {
            // Calculate the cutoff time
            let cutoff_secs = older_than.as_secs() as i64;
            let cutoff_sql = format!("datetime('now', '-{cutoff_secs} seconds')");

            // Find completed, non-ephemeral items older than cutoff
            let rows = diesel::sql_query(format!(
                "SELECT id, parent_id, title FROM work_items \
                 WHERE team_run_id = ?1 \
                 AND status = 'completed' \
                 AND is_ephemeral = 0 \
                 AND updated_at < {cutoff_sql}"
            ))
            .bind::<diesel::sql_types::Text, _>(team_run_id)
            .load::<CompactCandidateRow>(conn)?;

            if rows.is_empty() {
                return Ok(0);
            }

            // Group by parent_id
            let mut groups: HashMap<Option<i32>, Vec<CompactCandidateRow>> = HashMap::new();
            for row in rows {
                groups.entry(row.parent_id).or_default().push(row);
            }

            let mut total_compacted = 0;

            for (parent_id, items) in &groups {
                let count = items.len();
                let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
                let summary = if titles.len() <= 3 {
                    titles.join(", ")
                } else {
                    format!("{} and {} more", titles[..2].join(", "), titles.len() - 2)
                };

                // Insert compact digest
                diesel::insert_into(work_item_compacted::table)
                    .values(NewCompacted {
                        team_run_id,
                        parent_id: *parent_id,
                        summary: &summary,
                        item_count: count as i32,
                    })
                    .execute(conn)?;

                // Mark originals as compacted
                let ids: Vec<i32> = items.iter().map(|i| i.id).collect();
                diesel::update(work_items::table.filter(work_items::id.eq_any(&ids)))
                    .set((
                        work_items::status.eq(WorkStatus::Compacted.as_str()),
                        work_items::updated_at.eq(db::now_sql()),
                    ))
                    .execute(conn)?;

                total_compacted += count;
            }

            Ok(total_compacted)
        })
    }

    /// Get compacted digests for a run.
    pub fn get_digests(&self, team_run_id: &str) -> PersistenceResult<Vec<CompactDigest>> {
        self.db.with(|conn| {
            let rows = work_item_compacted::table
                .filter(work_item_compacted::team_run_id.eq(team_run_id))
                .order(work_item_compacted::created_at.desc())
                .load::<crate::models::CompactedRow>(conn)?;

            Ok(rows
                .into_iter()
                .map(|r| CompactDigest {
                    id: r.id,
                    team_run_id: r.team_run_id,
                    parent_id: r.parent_id,
                    summary: r.summary,
                    item_count: r.item_count,
                    created_at: r.created_at,
                })
                .collect())
        })
    }
}

/// A compacted digest entry.
#[derive(Debug, Clone)]
pub struct CompactDigest {
    pub id: i32,
    pub team_run_id: String,
    pub parent_id: Option<i32>,
    pub summary: String,
    pub item_count: i32,
    pub created_at: String,
}

#[derive(diesel::QueryableByName)]
struct CompactCandidateRow {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Integer>)]
    parent_id: Option<i32>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    title: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkItemStore;
    use crate::models::NewSession;
    use crate::schema::sessions;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    fn ensure_session(db: &Arc<Database>, key: &str) {
        db.with(|conn| {
            diesel::insert_into(sessions::table)
                .values(NewSession {
                    session_key: key,
                    selected_model: None,
                })
                .on_conflict(sessions::session_key)
                .do_nothing()
                .execute(conn)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_compact_empty() {
        let db = test_db();
        let store = CompactStore::new(db);
        let count = store.compact("run1", Duration::from_secs(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_compact_marks_items_compacted() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Old task", None).unwrap();
        wi.set_output(a, "done").unwrap();

        // Backdate the item so it's old enough
        db.with(|conn| {
            diesel::update(work_items::table.find(a))
                .set(work_items::updated_at.eq("2020-01-01 00:00:00"))
                .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let store = CompactStore::new(db.clone());
        let count = store.compact("run1", Duration::from_secs(0)).unwrap();
        assert_eq!(count, 1);

        // Verify the item is now compacted
        let item = wi.get(a).unwrap().unwrap();
        assert_eq!(item.status, WorkStatus::Compacted);
    }

    #[test]
    fn test_compact_creates_digest() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Task A", None).unwrap();
        let b = wi.create("sess1", "run1", "Task B", None).unwrap();
        wi.set_output(a, "done").unwrap();
        wi.set_output(b, "done").unwrap();

        // Backdate
        db.with(|conn| {
            diesel::update(work_items::table.filter(work_items::team_run_id.eq("run1")))
                .set(work_items::updated_at.eq("2020-01-01 00:00:00"))
                .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let store = CompactStore::new(db);
        store.compact("run1", Duration::from_secs(0)).unwrap();

        let digests = store.get_digests("run1").unwrap();
        assert_eq!(digests.len(), 1);
        assert_eq!(digests[0].item_count, 2);
    }

    #[test]
    fn test_compact_excludes_recent() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Recent task", None).unwrap();
        wi.set_output(a, "done").unwrap();

        let store = CompactStore::new(db);
        // Use a very long duration so nothing qualifies
        let count = store.compact("run1", Duration::from_secs(999999)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_compact_excludes_ephemeral() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create_wisp("sess1", "run1", "Wisp", "agent").unwrap();
        wi.set_output(a, "done").unwrap();

        db.with(|conn| {
            diesel::update(work_items::table.find(a))
                .set(work_items::updated_at.eq("2020-01-01 00:00:00"))
                .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let store = CompactStore::new(db);
        let count = store.compact("run1", Duration::from_secs(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_compacted_status_roundtrip() {
        assert_eq!(WorkStatus::Compacted.as_str(), "compacted");
        assert_eq!(
            WorkStatus::parse("compacted").unwrap(),
            WorkStatus::Compacted
        );
    }
}
