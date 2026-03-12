//! Work item relationships and DAG operations.
//!
//! Provides typed relations between work items (blocks, depends_on, etc.)
//! with cycle detection to maintain a valid DAG.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use diesel::prelude::*;

use crate::db::Database;
use crate::db_enum::db_enum;
use crate::error::{PersistenceError, PersistenceResult};
use crate::models::{NewRelation, RelationRow};
use crate::schema::{work_item_relations, work_items};

db_enum! {
    /// Type of relationship between work items.
    pub enum RelationType {
        Blocks => "blocks",
        DependsOn => "depends_on",
        RelatesTo => "relates_to",
        Duplicates => "duplicates",
    }
}

/// Relation between two work items.
#[derive(Debug, Clone)]
pub struct Relation {
    pub id: i32,
    pub from_item_id: i32,
    pub to_item_id: i32,
    pub relation_type: RelationType,
    pub created_at: String,
}

impl Relation {
    fn from_row(row: RelationRow) -> PersistenceResult<Self> {
        Ok(Self {
            id: row.id,
            from_item_id: row.from_item_id,
            to_item_id: row.to_item_id,
            relation_type: RelationType::parse(&row.relation_type)?,
            created_at: row.created_at,
        })
    }
}

/// Store for work item relationships.
pub struct RelationStore {
    db: Arc<Database>,
}

impl RelationStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Add a relation between two work items.
    /// Returns an error if either item is ephemeral or if this would create a cycle
    /// (for directional relations like Blocks/DependsOn).
    pub fn add_relation(
        &self,
        from_id: i32,
        to_id: i32,
        rel_type: RelationType,
    ) -> PersistenceResult<i32> {
        self.db.with(|conn| {
            // Check neither item is ephemeral
            let from_eph = work_items::table
                .find(from_id)
                .select(work_items::is_ephemeral)
                .first::<i32>(conn)
                .optional()?
                .ok_or_else(|| {
                    PersistenceError::InvalidEnumValue(format!("work item {from_id} not found"))
                })?;
            let to_eph = work_items::table
                .find(to_id)
                .select(work_items::is_ephemeral)
                .first::<i32>(conn)
                .optional()?
                .ok_or_else(|| {
                    PersistenceError::InvalidEnumValue(format!("work item {to_id} not found"))
                })?;

            if from_eph != 0 || to_eph != 0 {
                return Err(PersistenceError::InvalidEnumValue(
                    "ephemeral items cannot have relationships".into(),
                ));
            }

            // Check for cycles on directional relations
            if matches!(rel_type, RelationType::Blocks | RelationType::DependsOn) {
                if self.has_cycle_inner(conn, from_id, to_id, &rel_type)? {
                    return Err(PersistenceError::InvalidEnumValue(
                        "adding this relation would create a cycle".into(),
                    ));
                }
            }

            diesel::insert_into(work_item_relations::table)
                .values(NewRelation {
                    from_item_id: from_id,
                    to_item_id: to_id,
                    relation_type: rel_type.as_str(),
                })
                .execute(conn)?;

            let id = diesel::select(diesel::dsl::sql::<diesel::sql_types::Integer>(
                "last_insert_rowid()",
            ))
            .get_result::<i32>(conn)?;

            Ok(id)
        })
    }

    /// Remove a specific relation.
    pub fn remove_relation(
        &self,
        from_id: i32,
        to_id: i32,
        rel_type: RelationType,
    ) -> PersistenceResult<()> {
        self.db.with(|conn| {
            diesel::delete(
                work_item_relations::table
                    .filter(work_item_relations::from_item_id.eq(from_id))
                    .filter(work_item_relations::to_item_id.eq(to_id))
                    .filter(work_item_relations::relation_type.eq(rel_type.as_str())),
            )
            .execute(conn)?;
            Ok(())
        })
    }

    /// Get items that block the given item (items with a "blocks" relation TO this item).
    pub fn get_blockers(&self, item_id: i32) -> PersistenceResult<Vec<Relation>> {
        self.db.with(|conn| {
            let rows = work_item_relations::table
                .filter(work_item_relations::to_item_id.eq(item_id))
                .filter(work_item_relations::relation_type.eq(RelationType::Blocks.as_str()))
                .load::<RelationRow>(conn)?;
            rows.into_iter()
                .map(Relation::from_row)
                .collect::<Result<_, _>>()
        })
    }

    /// Get items that depend on the given item.
    pub fn get_dependents(&self, item_id: i32) -> PersistenceResult<Vec<Relation>> {
        self.db.with(|conn| {
            let rows = work_item_relations::table
                .filter(work_item_relations::from_item_id.eq(item_id))
                .filter(
                    work_item_relations::relation_type.eq(RelationType::DependsOn.as_str()),
                )
                .load::<RelationRow>(conn)?;
            rows.into_iter()
                .map(Relation::from_row)
                .collect::<Result<_, _>>()
        })
    }

    /// Check if adding from_id -> to_id would create a cycle.
    /// Uses BFS: check if to_id can reach from_id through existing edges.
    fn has_cycle_inner(
        &self,
        conn: &mut diesel::SqliteConnection,
        from_id: i32,
        to_id: i32,
        rel_type: &RelationType,
    ) -> PersistenceResult<bool> {
        // If from == to, it's a self-loop
        if from_id == to_id {
            return Ok(true);
        }

        // BFS from to_id, following same-type edges, checking if we reach from_id
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(to_id);
        visited.insert(to_id);

        while let Some(current) = queue.pop_front() {
            let neighbors = work_item_relations::table
                .filter(work_item_relations::from_item_id.eq(current))
                .filter(work_item_relations::relation_type.eq(rel_type.as_str()))
                .select(work_item_relations::to_item_id)
                .load::<i32>(conn)?;

            for next in neighbors {
                if next == from_id {
                    return Ok(true);
                }
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }

        Ok(false)
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

    fn create_item(db: &Arc<Database>, title: &str) -> i32 {
        let store = crate::WorkItemStore::new(db.clone());
        store.create("sess1", "run1", title, None).unwrap()
    }

    #[test]
    fn test_add_blocks_relation() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let a = create_item(&db, "Task A");
        let b = create_item(&db, "Task B");

        let store = RelationStore::new(db);
        let id = store.add_relation(a, b, RelationType::Blocks).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_add_depends_on_relation() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let a = create_item(&db, "Task A");
        let b = create_item(&db, "Task B");

        let store = RelationStore::new(db);
        store.add_relation(a, b, RelationType::DependsOn).unwrap();

        let deps = store.get_dependents(a).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].to_item_id, b);
    }

    #[test]
    fn test_detect_direct_cycle() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let a = create_item(&db, "Task A");
        let b = create_item(&db, "Task B");

        let store = RelationStore::new(db);
        store.add_relation(a, b, RelationType::Blocks).unwrap();

        let err = store.add_relation(b, a, RelationType::Blocks).unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[test]
    fn test_detect_transitive_cycle() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let a = create_item(&db, "A");
        let b = create_item(&db, "B");
        let c = create_item(&db, "C");

        let store = RelationStore::new(db);
        store.add_relation(a, b, RelationType::Blocks).unwrap();
        store.add_relation(b, c, RelationType::Blocks).unwrap();

        let err = store.add_relation(c, a, RelationType::Blocks).unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[test]
    fn test_allow_non_cyclic_graph() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let a = create_item(&db, "A");
        let b = create_item(&db, "B");
        let c = create_item(&db, "C");

        let store = RelationStore::new(db);
        store.add_relation(a, b, RelationType::Blocks).unwrap();
        store.add_relation(a, c, RelationType::Blocks).unwrap();
        store.add_relation(b, c, RelationType::Blocks).unwrap();
        // This is a DAG (diamond), should be fine
    }

    #[test]
    fn test_remove_relation() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let a = create_item(&db, "A");
        let b = create_item(&db, "B");

        let store = RelationStore::new(db);
        store.add_relation(a, b, RelationType::Blocks).unwrap();
        let blockers = store.get_blockers(b).unwrap();
        assert_eq!(blockers.len(), 1);

        store
            .remove_relation(a, b, RelationType::Blocks)
            .unwrap();
        let blockers = store.get_blockers(b).unwrap();
        assert!(blockers.is_empty());
    }

    #[test]
    fn test_get_blockers() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let a = create_item(&db, "A");
        let b = create_item(&db, "B");
        let c = create_item(&db, "C");

        let store = RelationStore::new(db);
        store.add_relation(a, c, RelationType::Blocks).unwrap();
        store.add_relation(b, c, RelationType::Blocks).unwrap();

        let blockers = store.get_blockers(c).unwrap();
        assert_eq!(blockers.len(), 2);
    }

    #[test]
    fn test_ephemeral_items_rejected() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let item_store = crate::WorkItemStore::new(db.clone());
        let a = item_store.create("sess1", "run1", "Normal", None).unwrap();
        let b = item_store
            .create_wisp("sess1", "run1", "Wisp", "agent")
            .unwrap();

        let store = RelationStore::new(db);
        let err = store.add_relation(a, b, RelationType::Blocks).unwrap_err();
        assert!(err.to_string().contains("ephemeral"));
    }

    #[test]
    fn test_self_loop_detected() {
        let db = test_db();
        ensure_session(&db, "sess1");
        let a = create_item(&db, "A");

        let store = RelationStore::new(db);
        let err = store.add_relation(a, a, RelationType::Blocks).unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[test]
    fn test_relation_type_roundtrip() {
        for rt in [
            RelationType::Blocks,
            RelationType::DependsOn,
            RelationType::RelatesTo,
            RelationType::Duplicates,
        ] {
            assert_eq!(RelationType::parse(rt.as_str()).unwrap(), rt);
        }
    }
}
