use chrono::{DateTime, Utc};
use std::collections::BTreeSet;

use crate::work_item::{Priority, WorkItem};

// ── Trait ──────────────────────────────────────────────

/// Conflict-free merge of two diverged values.
///
/// Implementations must satisfy:
/// - Commutativity: a.merge(b) == b.merge(a)
/// - Associativity: a.merge(b.merge(c)) == a.merge(b).merge(c)
/// - Idempotency:   a.merge(a) == a
pub trait Mergeable {
    fn merge(&self, other: &Self) -> Self;
}

// ── LWW Register ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LwwField<T> {
    pub value: T,
    pub updated_at: DateTime<Utc>,
}

impl<T: Clone> Mergeable for LwwField<T> {
    fn merge(&self, other: &Self) -> Self {
        if self.updated_at >= other.updated_at {
            self.clone()
        } else {
            other.clone()
        }
    }
}

// ── Priority: Max-register ────────────────────────────

impl Mergeable for Priority {
    fn merge(&self, other: &Self) -> Self {
        std::cmp::max(*self, *other)
    }
}

// ── Tags: G-Set (grow-only union) ─────────────────────

pub fn merge_tags(a: &[String], b: &[String]) -> Vec<String> {
    let mut union: BTreeSet<&str> = BTreeSet::new();
    for t in a.iter().chain(b.iter()) {
        union.insert(t.as_str());
    }
    union.into_iter().map(String::from).collect()
}

// ── Result types ──────────────────────────────────────

pub struct MergeResult {
    pub merged_items: Vec<MergedItem>,
    pub commit_id: u64,
}

pub struct MergedItem {
    pub item_id: i64,
    pub item: WorkItem,
    pub convergences: Vec<Convergence>,
}

pub struct Convergence {
    pub field: &'static str,
    pub branch_value: String,
    pub main_value: String,
    pub converged_to: String,
    pub strategy: MergeStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    OneSided,
    MaxRegister,
    GrowSet,
    LastWriteWins,
}

// ── 3-way merge ───────────────────────────────────────

pub fn merge_work_item(base: &WorkItem, branch: &WorkItem, main: &WorkItem) -> MergedItem {
    let mut convergences = Vec::new();

    let status = merge_lww(
        "status",
        &base.status,
        &branch.status,
        &main.status,
        branch.updated_at,
        main.updated_at,
        |s| format!("{s:?}"),
        &mut convergences,
    );

    let priority = merge_max(
        "priority",
        &base.priority,
        &branch.priority,
        &main.priority,
        &mut convergences,
    );

    let tags = merge_grow_set(
        "tags",
        &base.tags,
        &branch.tags,
        &main.tags,
        &mut convergences,
    );

    let claimed_by = merge_lww(
        "claimed_by",
        &base.claimed_by,
        &branch.claimed_by,
        &main.claimed_by,
        branch.updated_at,
        main.updated_at,
        |c| format!("{c:?}"),
        &mut convergences,
    );

    let merged_item = WorkItem {
        id: base.id,
        title: base.title.clone(),
        description: base.description.clone(),
        created_by: base.created_by.clone(),
        created_at: base.created_at,
        status,
        priority,
        tags,
        claimed_by,
        updated_at: std::cmp::max(branch.updated_at, main.updated_at),
    };

    MergedItem {
        item_id: base.id,
        item: merged_item,
        convergences,
    }
}

// ── 3-way merge helpers ───────────────────────────────

#[allow(clippy::too_many_arguments)]
fn merge_lww<T: Clone + PartialEq>(
    field: &'static str,
    base: &T,
    branch: &T,
    main: &T,
    branch_ts: DateTime<Utc>,
    main_ts: DateTime<Utc>,
    fmt: impl Fn(&T) -> String,
    convergences: &mut Vec<Convergence>,
) -> T {
    let branch_changed = branch != base;
    let main_changed = main != base;

    match (branch_changed, main_changed) {
        (false, false) => base.clone(),
        (true, false) => {
            convergences.push(Convergence {
                field,
                branch_value: fmt(branch),
                main_value: fmt(main),
                converged_to: fmt(branch),
                strategy: MergeStrategy::OneSided,
            });
            branch.clone()
        }
        (false, true) => {
            convergences.push(Convergence {
                field,
                branch_value: fmt(branch),
                main_value: fmt(main),
                converged_to: fmt(main),
                strategy: MergeStrategy::OneSided,
            });
            main.clone()
        }
        (true, true) => {
            let winner = if branch_ts >= main_ts { branch } else { main };
            convergences.push(Convergence {
                field,
                branch_value: fmt(branch),
                main_value: fmt(main),
                converged_to: fmt(winner),
                strategy: MergeStrategy::LastWriteWins,
            });
            winner.clone()
        }
    }
}

fn merge_max<T: Clone + PartialEq + Ord + std::fmt::Debug>(
    field: &'static str,
    base: &T,
    branch: &T,
    main: &T,
    convergences: &mut Vec<Convergence>,
) -> T {
    let branch_changed = branch != base;
    let main_changed = main != base;

    match (branch_changed, main_changed) {
        (false, false) => base.clone(),
        (true, false) => {
            convergences.push(Convergence {
                field,
                branch_value: format!("{branch:?}"),
                main_value: format!("{main:?}"),
                converged_to: format!("{branch:?}"),
                strategy: MergeStrategy::OneSided,
            });
            branch.clone()
        }
        (false, true) => {
            convergences.push(Convergence {
                field,
                branch_value: format!("{branch:?}"),
                main_value: format!("{main:?}"),
                converged_to: format!("{main:?}"),
                strategy: MergeStrategy::OneSided,
            });
            main.clone()
        }
        (true, true) => {
            let winner = std::cmp::max(branch, main);
            convergences.push(Convergence {
                field,
                branch_value: format!("{branch:?}"),
                main_value: format!("{main:?}"),
                converged_to: format!("{winner:?}"),
                strategy: MergeStrategy::MaxRegister,
            });
            winner.clone()
        }
    }
}

fn merge_grow_set(
    field: &'static str,
    base: &[String],
    branch: &[String],
    main: &[String],
    convergences: &mut Vec<Convergence>,
) -> Vec<String> {
    let branch_changed = branch != base;
    let main_changed = main != base;

    match (branch_changed, main_changed) {
        (false, false) => base.to_vec(),
        (true, false) => {
            convergences.push(Convergence {
                field,
                branch_value: format!("{branch:?}"),
                main_value: format!("{main:?}"),
                converged_to: format!("{branch:?}"),
                strategy: MergeStrategy::OneSided,
            });
            branch.to_vec()
        }
        (false, true) => {
            convergences.push(Convergence {
                field,
                branch_value: format!("{branch:?}"),
                main_value: format!("{main:?}"),
                converged_to: format!("{main:?}"),
                strategy: MergeStrategy::OneSided,
            });
            main.to_vec()
        }
        (true, true) => {
            let merged = merge_tags(branch, main);
            convergences.push(Convergence {
                field,
                branch_value: format!("{branch:?}"),
                main_value: format!("{main:?}"),
                converged_to: format!("{merged:?}"),
                strategy: MergeStrategy::GrowSet,
            });
            merged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_item::{RigId, Status};
    use chrono::TimeZone;

    #[test]
    fn priority_merge_takes_higher_urgency() {
        assert_eq!(Priority::P2.merge(&Priority::P0), Priority::P0);
        assert_eq!(Priority::P0.merge(&Priority::P2), Priority::P0);
        assert_eq!(Priority::P1.merge(&Priority::P1), Priority::P1);
    }

    #[test]
    fn tags_merge_is_union() {
        let a = vec!["rust".to_string(), "board".to_string()];
        let b = vec!["board".to_string(), "cow".to_string()];
        let merged = merge_tags(&a, &b);
        assert_eq!(merged, vec!["board", "cow", "rust"]);
    }

    #[test]
    fn tags_merge_is_commutative() {
        let a = vec!["x".to_string()];
        let b = vec!["y".to_string()];
        assert_eq!(merge_tags(&a, &b), merge_tags(&b, &a));
    }

    #[test]
    fn lww_field_takes_later_timestamp() {
        let earlier = LwwField {
            value: "old".to_string(),
            updated_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        };
        let later = LwwField {
            value: "new".to_string(),
            updated_at: Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap(),
        };
        assert_eq!(earlier.merge(&later).value, "new");
        assert_eq!(later.merge(&earlier).value, "new");
    }

    #[test]
    fn lww_field_tie_goes_to_self() {
        let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let a = LwwField {
            value: "a".to_string(),
            updated_at: ts,
        };
        let b = LwwField {
            value: "b".to_string(),
            updated_at: ts,
        };
        assert_eq!(a.merge(&b).value, "a");
    }

    #[test]
    fn lww_field_idempotent() {
        let f = LwwField {
            value: "same".to_string(),
            updated_at: Utc::now(),
        };
        assert_eq!(f.merge(&f).value, "same");
    }

    // ── 3-way merge tests ─────────────────────────────

    fn make_item(id: i64, status: Status, priority: Priority, tags: Vec<&str>) -> WorkItem {
        WorkItem {
            id,
            title: format!("Item {id}"),
            description: String::new(),
            created_by: RigId::new("test"),
            created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            status,
            priority,
            tags: tags.into_iter().map(String::from).collect(),
            claimed_by: None,
            updated_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn merge_no_changes_returns_base() {
        let base = make_item(1, Status::Open, Priority::P1, vec!["a"]);
        let result = merge_work_item(&base, &base, &base);
        assert!(result.convergences.is_empty());
        assert_eq!(result.item.status, Status::Open);
    }

    #[test]
    fn merge_one_side_changed_takes_that_side() {
        let base = make_item(1, Status::Open, Priority::P1, vec!["a"]);
        let mut branch = base.clone();
        branch.status = Status::Claimed;
        branch.claimed_by = Some(RigId::new("alice"));
        branch.updated_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap();

        let result = merge_work_item(&base, &branch, &base);
        assert_eq!(result.item.status, Status::Claimed);
        assert_eq!(result.item.claimed_by, Some(RigId::new("alice")));
        assert_eq!(result.convergences.len(), 2);
        assert!(
            result
                .convergences
                .iter()
                .all(|c| c.strategy == MergeStrategy::OneSided)
        );
    }

    #[test]
    fn merge_both_sides_changed_status_uses_lww() {
        let base = make_item(1, Status::Claimed, Priority::P1, vec![]);
        let mut branch = base.clone();
        branch.status = Status::Done;
        branch.updated_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 2, 0).unwrap();
        let mut main = base.clone();
        main.status = Status::Stuck;
        main.updated_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap();

        let result = merge_work_item(&base, &branch, &main);
        assert_eq!(result.item.status, Status::Done);
        let status_conv = result
            .convergences
            .iter()
            .find(|c| c.field == "status")
            .unwrap();
        assert_eq!(status_conv.strategy, MergeStrategy::LastWriteWins);
    }

    #[test]
    fn merge_both_sides_changed_priority_uses_max() {
        let base = make_item(1, Status::Open, Priority::P2, vec![]);
        let mut branch = base.clone();
        branch.priority = Priority::P1;
        branch.updated_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap();
        let mut main = base.clone();
        main.priority = Priority::P0;
        main.updated_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap();

        let result = merge_work_item(&base, &branch, &main);
        assert_eq!(result.item.priority, Priority::P0);
    }

    #[test]
    fn merge_both_sides_changed_tags_uses_union() {
        let base = make_item(1, Status::Open, Priority::P1, vec!["shared"]);
        let mut branch = base.clone();
        branch.tags.push("branch-tag".to_string());
        let mut main = base.clone();
        main.tags.push("main-tag".to_string());

        let result = merge_work_item(&base, &branch, &main);
        assert_eq!(result.item.tags, vec!["branch-tag", "main-tag", "shared"]);
        let tags_conv = result
            .convergences
            .iter()
            .find(|c| c.field == "tags")
            .unwrap();
        assert_eq!(tags_conv.strategy, MergeStrategy::GrowSet);
    }
}
