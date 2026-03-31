// Shared test fixtures for opengoose-board.
//
// Consolidates duplicated WorkItem construction patterns found across
// store/mod.rs, merge.rs, beads.rs, and work_item.rs tests.
//
// For Board + PostWorkItem + stamp helpers, see test_helpers.rs.

use crate::work_item::{Priority, RigId, Status, WorkItem};
use chrono::{TimeZone, Utc};

/// Deterministic base timestamp for reproducible tests.
pub fn fixed_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}

/// Minimal WorkItem with sensible defaults (Open, P1, no tags).
pub fn make_work_item(id: i64) -> WorkItem {
    WorkItem {
        id,
        title: format!("Test item {id}"),
        description: String::new(),
        created_by: RigId::new("test"),
        created_at: fixed_time(),
        status: Status::Open,
        priority: Priority::P1,
        tags: vec![],
        claimed_by: None,
        updated_at: fixed_time(),
        parent_id: None,
    }
}

/// WorkItem with explicit status, priority, and tags.
pub fn make_work_item_full(
    id: i64,
    status: Status,
    priority: Priority,
    tags: Vec<&str>,
) -> WorkItem {
    WorkItem {
        id,
        title: format!("Test item {id}"),
        description: String::new(),
        created_by: RigId::new("test"),
        created_at: fixed_time(),
        status,
        priority,
        tags: tags.into_iter().map(String::from).collect(),
        claimed_by: None,
        updated_at: fixed_time(),
        parent_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_work_item_has_expected_defaults() {
        let item = make_work_item(42);
        assert_eq!(item.id, 42);
        assert_eq!(item.status, Status::Open);
        assert_eq!(item.priority, Priority::P1);
        assert!(item.tags.is_empty());
        assert!(item.claimed_by.is_none());
    }

    #[test]
    fn make_work_item_full_sets_fields() {
        let item = make_work_item_full(1, Status::Claimed, Priority::P0, vec!["bug", "urgent"]);
        assert_eq!(item.status, Status::Claimed);
        assert_eq!(item.priority, Priority::P0);
        assert_eq!(item.tags, vec!["bug", "urgent"]);
    }
}
