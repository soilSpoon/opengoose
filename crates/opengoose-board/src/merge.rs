// Merge — 셀 레벨 충돌 해결 (Dolt 영감)
//
// 3-way merge: base(분기 시점) vs source(브랜치) vs dest(main)
//
// 4가지 규칙 (Beads 머지 드라이버와 동일):
// 1. 한쪽만 고침 → 고친 쪽 반영
// 2. 스칼라 양쪽 고침 → 나중에 쓴 쪽 (updated_at 비교)
// 3. 배열 양쪽 고침 → 합치기 (union, 중복 제거) — Phase 2+ (tags 추가 시)
// 4. status/priority → 더 높은 쪽

use crate::work_item::WorkItem;

/// 단일 WorkItem의 3-way merge.
/// base: 분기 시점, source: 브랜치, dest: main 현재.
pub fn merge_work_item(base: &WorkItem, source: &WorkItem, dest: &WorkItem) -> WorkItem {
    let mut merged = dest.clone();

    // 규칙 4: status — 더 진행된 쪽, priority — 더 긴급한 쪽
    merged.status = merge_ord(base.status, source.status, dest.status);
    merged.priority = merge_ord(base.priority, source.priority, dest.priority);

    // 규칙 1 & 2: claimed_by
    merged.claimed_by =
        merge_scalar(&base.claimed_by, &source.claimed_by, &dest.claimed_by, source, dest);

    // updated_at: 항상 더 큰 값
    merged.updated_at = std::cmp::max(source.updated_at, dest.updated_at);

    merged
}

/// 3-way merge for Ord types: 한쪽만 바뀜 → 바뀐 쪽, 양쪽 바뀜 → 더 큰 쪽.
fn merge_ord<T: Copy + Eq + Ord>(base: T, source: T, dest: T) -> T {
    if source == base {
        dest
    } else if dest == base {
        source
    } else {
        std::cmp::max(source, dest)
    }
}

fn merge_scalar<T: Clone + PartialEq>(
    base: &T,
    source: &T,
    dest: &T,
    source_item: &WorkItem,
    dest_item: &WorkItem,
) -> T {
    if source == base {
        // source 안 바뀜 → dest 반영
        dest.clone()
    } else if dest != base && dest_item.updated_at > source_item.updated_at {
        // 양쪽 바뀜, dest가 더 최근
        dest.clone()
    } else {
        // dest 안 바뀜 또는 source가 더 최근
        source.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_item::{Priority, RigId, Status};
    use chrono::Utc;

    fn make_item(id: i64) -> WorkItem {
        let now = Utc::now();
        WorkItem {
            id,
            title: format!("Task {id}"),
            description: String::new(),
            created_by: RigId::new("test"),
            created_at: now,
            status: Status::Open,
            priority: Priority::P1,
            claimed_by: None,
            updated_at: now,
        }
    }

    #[test]
    fn rule1_one_side_changed_status() {
        let base = make_item(1);
        let source = base.clone();
        let mut dest = base.clone();
        dest.status = Status::Claimed;

        let merged = merge_work_item(&base, &source, &dest);
        assert_eq!(merged.status, Status::Claimed);
    }

    #[test]
    fn rule4_both_changed_status_higher_wins() {
        let base = make_item(1);
        let mut source = base.clone();
        source.status = Status::Claimed;
        let mut dest = base.clone();
        dest.status = Status::Done;

        let merged = merge_work_item(&base, &source, &dest);
        assert_eq!(merged.status, Status::Done);
    }

    #[test]
    fn rule4_both_changed_priority_more_urgent_wins() {
        let base = make_item(1);
        let mut source = base.clone();
        source.priority = Priority::P1;
        let mut dest = base.clone();
        dest.priority = Priority::P0;

        let merged = merge_work_item(&base, &source, &dest);
        assert_eq!(merged.priority, Priority::P0);
    }

    #[test]
    fn rule2_claimed_by_later_wins() {
        let base = make_item(1);
        let mut source = base.clone();
        source.claimed_by = Some(RigId::new("rig-a"));
        source.updated_at = Utc::now();

        let mut dest = base.clone();
        dest.claimed_by = Some(RigId::new("rig-b"));
        dest.updated_at = source.updated_at - chrono::Duration::seconds(1);

        let merged = merge_work_item(&base, &source, &dest);
        assert_eq!(merged.claimed_by, Some(RigId::new("rig-a")));
    }

    #[test]
    fn rule1_claimed_by_one_side() {
        let base = make_item(1);
        let source = base.clone();
        let mut dest = base.clone();
        dest.claimed_by = Some(RigId::new("rig-a"));

        let merged = merge_work_item(&base, &source, &dest);
        assert_eq!(merged.claimed_by, Some(RigId::new("rig-a")));
    }

    #[test]
    fn updated_at_always_max() {
        let base = make_item(1);
        let mut source = base.clone();
        source.updated_at = Utc::now() + chrono::Duration::seconds(10);
        let dest = base.clone();

        let merged = merge_work_item(&base, &source, &dest);
        assert_eq!(merged.updated_at, source.updated_at);
    }
}
