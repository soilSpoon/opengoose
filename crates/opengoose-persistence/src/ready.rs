//! The `ready()` algorithm: returns pending work items that are unblocked
//! and have all dependencies satisfied.

use std::sync::Arc;

use diesel::prelude::*;

use crate::db::Database;
use crate::error::PersistenceResult;
use crate::work_items::{WorkItem, WorkStatus};

/// Options for the `ready()` query.
#[derive(Debug, Clone)]
pub struct ReadyOptions {
    /// Maximum number of items to return (default 10).
    pub batch_size: i64,
    /// If true, include items already assigned to someone.
    pub include_assigned: bool,
}

impl Default for ReadyOptions {
    fn default() -> Self {
        Self {
            batch_size: 10,
            include_assigned: false,
        }
    }
}

/// Store for ready() operations.
pub struct ReadyStore {
    db: Arc<Database>,
}

impl ReadyStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get work items that are ready to be worked on.
    ///
    /// An item is "ready" if it:
    /// 1. Is NOT ephemeral
    /// 2. Is in "pending" status
    /// 3. Is NOT blocked by any open item (via "blocks" relations)
    /// 4. Has all "depends_on" relations satisfied (targets completed)
    /// 5. Is NOT already assigned (unless include_assigned is true)
    ///
    /// Results are ordered by priority ASC, created_at ASC.
    pub fn ready(
        &self,
        team_run_id: &str,
        options: &ReadyOptions,
    ) -> PersistenceResult<Vec<WorkItem>> {
        self.db.with(|conn| {
            // Use raw SQL for the NOT EXISTS subqueries which are hard to express in Diesel
            let sql = if options.include_assigned {
                "SELECT * FROM work_items \
                     WHERE team_run_id = ?1 \
                     AND is_ephemeral = 0 \
                     AND status = 'pending' \
                     AND NOT EXISTS ( \
                         SELECT 1 FROM work_item_relations r \
                         INNER JOIN work_items blocker ON blocker.id = r.from_item_id \
                         WHERE r.to_item_id = work_items.id \
                         AND r.relation_type = 'blocks' \
                         AND blocker.status NOT IN ('completed', 'cancelled', 'compacted') \
                     ) \
                     AND NOT EXISTS ( \
                         SELECT 1 FROM work_item_relations r \
                         INNER JOIN work_items dep ON dep.id = r.to_item_id \
                         WHERE r.from_item_id = work_items.id \
                         AND r.relation_type = 'depends_on' \
                         AND dep.status NOT IN ('completed') \
                     ) \
                     AND status != 'compacted' \
                     ORDER BY priority ASC, created_at ASC \
                     LIMIT ?2"
                    .to_string()
            } else {
                "SELECT * FROM work_items \
                     WHERE team_run_id = ?1 \
                     AND is_ephemeral = 0 \
                     AND status = 'pending' \
                     AND assigned_to IS NULL \
                     AND NOT EXISTS ( \
                         SELECT 1 FROM work_item_relations r \
                         INNER JOIN work_items blocker ON blocker.id = r.from_item_id \
                         WHERE r.to_item_id = work_items.id \
                         AND r.relation_type = 'blocks' \
                         AND blocker.status NOT IN ('completed', 'cancelled', 'compacted') \
                     ) \
                     AND NOT EXISTS ( \
                         SELECT 1 FROM work_item_relations r \
                         INNER JOIN work_items dep ON dep.id = r.to_item_id \
                         WHERE r.from_item_id = work_items.id \
                         AND r.relation_type = 'depends_on' \
                         AND dep.status NOT IN ('completed') \
                     ) \
                     AND status != 'compacted' \
                     ORDER BY priority ASC, created_at ASC \
                     LIMIT ?2"
                    .to_string()
            };

            let rows = diesel::sql_query(sql)
                .bind::<diesel::sql_types::Text, _>(team_run_id)
                .bind::<diesel::sql_types::BigInt, _>(options.batch_size)
                .load::<ReadyWorkItemRow>(conn)?;

            rows.into_iter()
                .map(|r| r.into_work_item())
                .collect::<Result<_, _>>()
        })
    }
}

/// Raw queryable row for the ready() SQL query.
#[derive(diesel::QueryableByName)]
struct ReadyWorkItemRow {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    id: i32,
    #[diesel(sql_type = diesel::sql_types::Text)]
    session_key: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    team_run_id: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Integer>)]
    parent_id: Option<i32>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    title: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    description: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    status: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    assigned_to: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Integer>)]
    workflow_step: Option<i32>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    input: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    output: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    error: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    created_at: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    updated_at: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    hash_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    is_ephemeral: i32,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    priority: i32,
}

impl ReadyWorkItemRow {
    fn into_work_item(self) -> PersistenceResult<WorkItem> {
        Ok(WorkItem {
            id: self.id,
            session_key: self.session_key,
            team_run_id: self.team_run_id,
            parent_id: self.parent_id,
            title: self.title,
            description: self.description,
            status: WorkStatus::parse(&self.status)?,
            assigned_to: self.assigned_to,
            workflow_step: self.workflow_step,
            input: self.input,
            output: self.output,
            error: self.error,
            created_at: self.created_at,
            updated_at: self.updated_at,
            hash_id: self.hash_id,
            is_ephemeral: self.is_ephemeral != 0,
            priority: self.priority,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewSession;
    use crate::schema::{sessions, work_items};
    use crate::{RelationStore, RelationType, WorkItemStore};

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
    fn test_ready_empty() {
        let db = test_db();
        let store = ReadyStore::new(db);
        let items = store.ready("run1", &ReadyOptions::default()).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn test_ready_single_pending() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        wi.create("sess1", "run1", "Task A", None).unwrap();

        let store = ReadyStore::new(db);
        let items = store.ready("run1", &ReadyOptions::default()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Task A");
    }

    #[test]
    fn test_ready_excludes_blocked() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Blocker", None).unwrap();
        let b = wi.create("sess1", "run1", "Blocked", None).unwrap();

        let rel = RelationStore::new(db.clone());
        rel.add_relation(a, b, RelationType::Blocks).unwrap();

        let store = ReadyStore::new(db);
        let items = store.ready("run1", &ReadyOptions::default()).unwrap();
        // Only "Blocker" should be ready, "Blocked" is blocked
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Blocker");
    }

    #[test]
    fn test_ready_unblocked_when_blocker_completed() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Blocker", None).unwrap();
        let b = wi.create("sess1", "run1", "Blocked", None).unwrap();

        let rel = RelationStore::new(db.clone());
        rel.add_relation(a, b, RelationType::Blocks).unwrap();

        // Complete the blocker
        wi.update_status(a, WorkStatus::Completed).unwrap();

        let store = ReadyStore::new(db);
        let items = store.ready("run1", &ReadyOptions::default()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Blocked");
    }

    #[test]
    fn test_ready_priority_ordering() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Low priority", None).unwrap();
        let b = wi.create("sess1", "run1", "High priority", None).unwrap();

        // Set priorities directly
        db.with(|conn| {
            diesel::update(work_items::table.find(a))
                .set(work_items::priority.eq(5))
                .execute(conn)?;
            diesel::update(work_items::table.find(b))
                .set(work_items::priority.eq(1))
                .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let store = ReadyStore::new(db);
        let items = store.ready("run1", &ReadyOptions::default()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "High priority");
        assert_eq!(items[1].title, "Low priority");
    }

    #[test]
    fn test_ready_batch_limit() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        for i in 0..5 {
            wi.create("sess1", "run1", &format!("Task {i}"), None)
                .unwrap();
        }

        let store = ReadyStore::new(db);
        let opts = ReadyOptions {
            batch_size: 3,
            ..Default::default()
        };
        let items = store.ready("run1", &opts).unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_ready_excludes_ephemeral() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        wi.create("sess1", "run1", "Normal", None).unwrap();
        wi.create_wisp("sess1", "run1", "Wisp", "agent").unwrap();

        let store = ReadyStore::new(db);
        let items = store.ready("run1", &ReadyOptions::default()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Normal");
    }

    #[test]
    fn test_ready_excludes_assigned() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Assigned", None).unwrap();
        wi.create("sess1", "run1", "Unassigned", None).unwrap();
        wi.assign(a, "agent", None).unwrap();

        let store = ReadyStore::new(db);
        let items = store.ready("run1", &ReadyOptions::default()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Unassigned");
    }

    #[test]
    fn test_ready_includes_assigned_when_option_set() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Task", None).unwrap();
        // Assign but keep status pending (manually)
        db.with(|conn| {
            diesel::update(work_items::table.find(a))
                .set(work_items::assigned_to.eq(Some("agent")))
                .execute(conn)?;
            Ok(())
        })
        .unwrap();

        let store = ReadyStore::new(db);
        let opts = ReadyOptions {
            include_assigned: true,
            ..Default::default()
        };
        let items = store.ready("run1", &opts).unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_ready_scoped_to_run() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        wi.create("sess1", "run1", "Run1 task", None).unwrap();
        wi.create("sess1", "run2", "Run2 task", None).unwrap();

        let store = ReadyStore::new(db);
        let items = store.ready("run1", &ReadyOptions::default()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Run1 task");
    }
}
