//! The `prime()` algorithm: generates minimal context strings for agent system prompts.
//!
//! Produces a compact summary of active, ready, recently completed, and blocked tasks
//! for injection into agent context. Targets <2000 tokens for 100 tasks.

use std::sync::Arc;

use diesel::prelude::*;

use crate::db::Database;
use crate::error::PersistenceResult;
use crate::schema::work_items;
use crate::work_items::WorkStatus;

/// Store for prime() operations.
pub struct PrimeStore {
    db: Arc<Database>,
}

impl PrimeStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Generate a context string for the given agent.
    ///
    /// Sections:
    /// 1. Active Tasks (assigned to this agent, in_progress)
    /// 2. Ready Tasks (pending, unblocked, unassigned)
    /// 3. Recently Completed (last 5)
    /// 4. Blocked items
    pub fn prime(&self, team_run_id: &str, agent_name: &str) -> PersistenceResult<String> {
        self.db.with(|conn| {
            let mut sections = Vec::new();

            // 1. Active Tasks (assigned to this agent)
            let active = work_items::table
                .filter(work_items::team_run_id.eq(team_run_id))
                .filter(work_items::assigned_to.eq(agent_name))
                .filter(work_items::status.eq(WorkStatus::InProgress.as_str()))
                .filter(work_items::is_ephemeral.eq(0))
                .order(work_items::priority.asc())
                .select((
                    work_items::hash_id,
                    work_items::title,
                    work_items::status,
                    work_items::priority,
                ))
                .load::<(Option<String>, String, String, i32)>(conn)?;

            if !active.is_empty() {
                let mut s = String::from("# Active Tasks (assigned to you)\n");
                for (hash_id, title, _status, _priority) in &active {
                    let id = hash_id.as_deref().unwrap_or("?");
                    s.push_str(&format!("- [{id}] {title} (in_progress)\n"));
                }
                sections.push(s);
            }

            // 2. Ready Tasks (pending, unassigned, not blocked)
            let ready_rows = diesel::sql_query(
                "SELECT hash_id, title, priority FROM work_items \
                 WHERE team_run_id = ?1 \
                 AND is_ephemeral = 0 \
                 AND status = 'pending' \
                 AND assigned_to IS NULL \
                 AND status != 'compacted' \
                 AND NOT EXISTS ( \
                     SELECT 1 FROM work_item_relations r \
                     INNER JOIN work_items blocker ON blocker.id = r.from_item_id \
                     WHERE r.to_item_id = work_items.id \
                     AND r.relation_type = 'blocks' \
                     AND blocker.status NOT IN ('completed', 'cancelled', 'compacted') \
                 ) \
                 ORDER BY priority ASC, created_at ASC \
                 LIMIT 10",
            )
            .bind::<diesel::sql_types::Text, _>(team_run_id)
            .load::<PrimeReadyRow>(conn)?;

            if !ready_rows.is_empty() {
                let mut s = String::from("# Ready Tasks (available)\n");
                for row in &ready_rows {
                    let id = row.hash_id.as_deref().unwrap_or("?");
                    s.push_str(&format!(
                        "- [{id}] {} (pending, priority: {})\n",
                        row.title, row.priority
                    ));
                }
                sections.push(s);
            }

            // 3. Recently Completed (last 5)
            let completed = work_items::table
                .filter(work_items::team_run_id.eq(team_run_id))
                .filter(work_items::status.eq(WorkStatus::Completed.as_str()))
                .filter(work_items::is_ephemeral.eq(0))
                .order(work_items::updated_at.desc())
                .limit(5)
                .select((
                    work_items::hash_id,
                    work_items::title,
                    work_items::updated_at,
                ))
                .load::<(Option<String>, String, String)>(conn)?;

            if !completed.is_empty() {
                let mut s = String::from("# Recently Completed\n");
                for (hash_id, title, updated) in &completed {
                    let id = hash_id.as_deref().unwrap_or("?");
                    s.push_str(&format!("- [{id}] {title} (completed, {updated})\n"));
                }
                sections.push(s);
            }

            // 4. Blocked items
            let blocked_rows = diesel::sql_query(
                "SELECT w.hash_id, w.title, GROUP_CONCAT(blocker.hash_id, ', ') as blocker_ids \
                 FROM work_items w \
                 INNER JOIN work_item_relations r ON r.to_item_id = w.id \
                 INNER JOIN work_items blocker ON blocker.id = r.from_item_id \
                 WHERE w.team_run_id = ?1 \
                 AND w.is_ephemeral = 0 \
                 AND w.status = 'pending' \
                 AND r.relation_type = 'blocks' \
                 AND blocker.status NOT IN ('completed', 'cancelled', 'compacted') \
                 GROUP BY w.id \
                 ORDER BY w.priority ASC \
                 LIMIT 10",
            )
            .bind::<diesel::sql_types::Text, _>(team_run_id)
            .load::<PrimeBlockedRow>(conn)?;

            if !blocked_rows.is_empty() {
                let mut s = String::from("# Blocked\n");
                for row in &blocked_rows {
                    let id = row.hash_id.as_deref().unwrap_or("?");
                    let blockers = row.blocker_ids.as_deref().unwrap_or("?");
                    s.push_str(&format!(
                        "- [{id}] {} (blocked by: {blockers})\n",
                        row.title
                    ));
                }
                sections.push(s);
            }

            if sections.is_empty() {
                Ok(String::from("# No active tasks\n"))
            } else {
                Ok(sections.join("\n"))
            }
        })
    }
}

#[derive(diesel::QueryableByName)]
struct PrimeReadyRow {
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    hash_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    title: String,
    #[diesel(sql_type = diesel::sql_types::Integer)]
    priority: i32,
}

#[derive(diesel::QueryableByName)]
struct PrimeBlockedRow {
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    hash_id: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Text)]
    title: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    blocker_ids: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RelationStore, RelationType, WorkItemStore};
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
    fn test_prime_empty() {
        let db = test_db();
        let store = PrimeStore::new(db);
        let result = store.prime("run1", "agent").unwrap();
        assert!(result.contains("No active tasks"));
    }

    #[test]
    fn test_prime_active_tasks() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Fix bug", None).unwrap();
        wi.assign(a, "coder", None).unwrap();

        let store = PrimeStore::new(db);
        let result = store.prime("run1", "coder").unwrap();
        assert!(result.contains("Active Tasks"));
        assert!(result.contains("Fix bug"));
        assert!(result.contains("in_progress"));
    }

    #[test]
    fn test_prime_ready_tasks() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        wi.create("sess1", "run1", "Available task", None).unwrap();

        let store = PrimeStore::new(db);
        let result = store.prime("run1", "agent").unwrap();
        assert!(result.contains("Ready Tasks"));
        assert!(result.contains("Available task"));
    }

    #[test]
    fn test_prime_completed_tasks() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Done task", None).unwrap();
        wi.update_status(a, WorkStatus::Completed).unwrap();

        let store = PrimeStore::new(db);
        let result = store.prime("run1", "agent").unwrap();
        assert!(result.contains("Recently Completed"));
        assert!(result.contains("Done task"));
    }

    #[test]
    fn test_prime_blocked_tasks() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "Blocker", None).unwrap();
        let b = wi.create("sess1", "run1", "Blocked item", None).unwrap();

        let rel = RelationStore::new(db.clone());
        rel.add_relation(a, b, RelationType::Blocks).unwrap();

        let store = PrimeStore::new(db);
        let result = store.prime("run1", "agent").unwrap();
        assert!(result.contains("Blocked"));
        assert!(result.contains("Blocked item"));
        assert!(result.contains("blocked by"));
    }

    #[test]
    fn test_prime_excludes_ephemeral() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        wi.create_wisp("sess1", "run1", "Wisp task", "agent")
            .unwrap();

        let store = PrimeStore::new(db);
        let result = store.prime("run1", "agent").unwrap();
        assert!(!result.contains("Wisp task"));
    }

    #[test]
    fn test_prime_has_hash_ids() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        wi.create("sess1", "run1", "Task with hash", None).unwrap();

        let store = PrimeStore::new(db);
        let result = store.prime("run1", "agent").unwrap();
        assert!(result.contains("[bd-"));
    }

    #[test]
    fn test_prime_scoped_to_agent() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let wi = WorkItemStore::new(db.clone());
        let a = wi.create("sess1", "run1", "My task", None).unwrap();
        let b = wi.create("sess1", "run1", "Other task", None).unwrap();
        wi.assign(a, "agent-a", None).unwrap();
        wi.assign(b, "agent-b", None).unwrap();

        let store = PrimeStore::new(db);
        let result = store.prime("run1", "agent-a").unwrap();
        assert!(result.contains("My task"));
        assert!(!result.contains("Other task"));
    }
}
