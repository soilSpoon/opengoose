use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use crate::work_item::{RigId, Status, WorkItem};

#[derive(Debug)]
pub struct Branch {
    pub(crate) name: RigId,
    pub(crate) data: Arc<BTreeMap<i64, WorkItem>>,
    pub(crate) base_data: Arc<BTreeMap<i64, WorkItem>>,
    pub(crate) base_commit: u64,
}

impl Branch {
    pub(crate) fn new(name: RigId, data: Arc<BTreeMap<i64, WorkItem>>, base_commit: u64) -> Self {
        let base_data = Arc::clone(&data);
        Self {
            name,
            data,
            base_data,
            base_commit,
        }
    }

    pub fn name(&self) -> &RigId {
        &self.name
    }

    pub fn get(&self, id: i64) -> Option<&WorkItem> {
        self.data.get(&id)
    }

    pub fn list(&self) -> impl Iterator<Item = &WorkItem> {
        self.data.values()
    }

    pub fn ready(&self, blocked_ids: &HashSet<i64>) -> Vec<&WorkItem> {
        let mut items: Vec<&WorkItem> = self
            .data
            .values()
            .filter(|item| item.status == Status::Open && !blocked_ids.contains(&item.id))
            .collect();
        items.sort_by(|a, b| b.priority.cmp(&a.priority));
        items
    }

    pub fn update(&mut self, id: i64, f: impl FnOnce(&mut WorkItem)) {
        if let Some(item) = Arc::make_mut(&mut self.data).get_mut(&id) {
            f(item);
        }
    }

    pub fn remove(&mut self, id: i64) {
        Arc::make_mut(&mut self.data).remove(&id);
    }
}

#[cfg(test)]
impl Branch {
    fn insert(&mut self, item: WorkItem) {
        Arc::make_mut(&mut self.data).insert(item.id, item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_item::Priority;
    use chrono::{TimeZone, Utc};

    fn sample_items() -> Arc<BTreeMap<i64, WorkItem>> {
        let mut map = BTreeMap::new();
        for id in 1..=3 {
            map.insert(
                id,
                WorkItem {
                    id,
                    title: format!("Item {id}"),
                    description: String::new(),
                    created_by: RigId::new("test"),
                    created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                    status: Status::Open,
                    priority: Priority::P1,
                    tags: vec![],
                    claimed_by: None,
                    updated_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                },
            );
        }
        Arc::new(map)
    }

    #[test]
    fn branch_get_reads_snapshot() {
        let data = sample_items();
        let branch = Branch::new(RigId::new("alice"), data, 0);
        assert!(branch.get(1).is_some());
        assert!(branch.get(99).is_none());
    }

    #[test]
    fn branch_list_returns_all() {
        let data = sample_items();
        let branch = Branch::new(RigId::new("alice"), data, 0);
        assert_eq!(branch.list().count(), 3);
    }

    #[test]
    fn branch_update_triggers_cow() {
        let data = sample_items();
        let original_ptr = Arc::as_ptr(&data);
        let mut branch = Branch::new(RigId::new("alice"), data.clone(), 0);

        branch.update(1, |item| {
            item.status = Status::Claimed;
            item.claimed_by = Some(RigId::new("alice"));
        });

        assert_ne!(Arc::as_ptr(&branch.data), original_ptr);
        assert_eq!(Arc::as_ptr(&branch.base_data), original_ptr);
        assert_eq!(data.get(&1).expect("get should succeed").status, Status::Open);
        assert_eq!(branch.get(1).expect("get should succeed").status, Status::Claimed);
    }

    #[test]
    fn branch_ready_excludes_blocked_and_non_open() {
        let data = sample_items();
        let mut branch = Branch::new(RigId::new("alice"), data, 0);
        branch.update(2, |item| item.status = Status::Claimed);

        let blocked: HashSet<i64> = [3].into_iter().collect();
        let ready = branch.ready(&blocked);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, 1);
    }

    #[test]
    fn branch_ready_sorted_by_priority_desc() {
        let mut map = BTreeMap::new();
        for (id, prio) in [(1, Priority::P2), (2, Priority::P0), (3, Priority::P1)] {
            map.insert(
                id,
                WorkItem {
                    id,
                    title: format!("Item {id}"),
                    description: String::new(),
                    created_by: RigId::new("test"),
                    created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                    status: Status::Open,
                    priority: prio,
                    tags: vec![],
                    claimed_by: None,
                    updated_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                },
            );
        }
        let branch = Branch::new(RigId::new("alice"), Arc::new(map), 0);
        let ready = branch.ready(&HashSet::new());
        assert_eq!(ready[0].id, 2); // P0
        assert_eq!(ready[1].id, 3); // P1
        assert_eq!(ready[2].id, 1); // P2
    }

    #[test]
    fn branch_insert_and_remove() {
        let data = sample_items();
        let mut branch = Branch::new(RigId::new("alice"), data, 0);

        let new_item = WorkItem {
            id: 99,
            title: "New".to_string(),
            description: String::new(),
            created_by: RigId::new("alice"),
            created_at: Utc::now(),
            status: Status::Open,
            priority: Priority::P0,
            tags: vec![],
            claimed_by: None,
            updated_at: Utc::now(),
        };
        branch.insert(new_item);
        assert_eq!(branch.list().count(), 4);

        branch.remove(99);
        assert_eq!(branch.list().count(), 3);
    }
}
