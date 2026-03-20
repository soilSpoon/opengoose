// Beads 알고리즘 — ready / prime / compact
//
// ready() = 블로킹 없는 작업 목록 (의존성 + 우선순위)
// prime() = 1-2K 토큰 컨텍스트 요약
// compact() = Phase 5

use crate::work_item::{RigId, Status, WorkItem};

/// ready() 결과에서 작업 목록을 필터링하는 로직.
///
/// 1. open 상태만
/// 2. 블로킹 의존성 없는 것만
/// 3. 우선순위 정렬 (P0 > P1 > P2)
pub fn filter_ready(
    items: impl Iterator<Item = WorkItem>,
    blocked_ids: &std::collections::HashSet<i64>,
) -> Vec<WorkItem> {
    let mut ready: Vec<WorkItem> = items
        .filter(|item| item.status == Status::Open && !blocked_ids.contains(&item.id))
        .collect();

    ready.sort_by(|a, b| b.priority.urgency().cmp(&a.priority.urgency()));
    ready
}

/// prime() — 에이전트 컨텍스트 요약. Phase 1: 최소 구현.
pub fn prime_summary(items: &[WorkItem], rig_id: &RigId) -> String {
    let (mut open, mut claimed, mut done) = (0usize, 0usize, 0usize);
    let mut recent_done: Vec<&WorkItem> = Vec::with_capacity(3);

    for item in items {
        match item.status {
            Status::Open => open += 1,
            Status::Claimed => claimed += 1,
            Status::Done => {
                done += 1;
                if recent_done.len() < 3 {
                    recent_done.push(item);
                }
            }
            _ => {}
        }
    }

    let mut summary = format!(
        "Board: {open} open, {claimed} claimed, {done} done\n\
         Rig: {rig_id}\n"
    );

    if !recent_done.is_empty() {
        summary.push_str("Recent:\n");
        for item in recent_done {
            summary.push_str(&format!("  #{} {}\n", item.id, item.title));
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Priority;
    use chrono::Utc;

    fn make_item(id: i64, status: Status, priority: Priority, title: &str) -> WorkItem {
        WorkItem {
            id,
            title: title.into(),
            description: String::new(),
            created_by: RigId::new("u1"),
            created_at: Utc::now(),
            status,
            priority,
            tags: vec![],
            claimed_by: None,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn filter_ready_removes_blocked_and_sorts_by_priority() {
        let items = vec![
            make_item(1, Status::Open, Priority::P2, "low"),
            make_item(2, Status::Open, Priority::P0, "high"),
            make_item(3, Status::Done, Priority::P0, "done"),
        ];
        let blocked = [3_i64].into_iter().collect();
        let ready = filter_ready(items.into_iter(), &blocked);
        let ids: Vec<_> = ready.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![2, 1]);
    }

    #[test]
    fn prime_summary_counts_and_recent_done() {
        let items = vec![
            make_item(1, Status::Open, Priority::P1, "open"),
            make_item(2, Status::Claimed, Priority::P1, "claimed"),
            make_item(3, Status::Done, Priority::P1, "done1"),
            make_item(4, Status::Done, Priority::P1, "done2"),
            make_item(5, Status::Done, Priority::P1, "done3"),
            make_item(6, Status::Done, Priority::P1, "done4"),
            make_item(7, Status::Stuck, Priority::P1, "stuck"),
        ];
        let summary = prime_summary(&items, &RigId::new("worker"));
        assert!(summary.contains("1 open"));
        assert!(summary.contains("1 claimed"));
        assert!(summary.contains("4 done"));
        assert!(summary.contains("Recent:"));
        assert!(summary.contains("#3"));
        assert!(summary.contains("#5"));
    }

    #[test]
    fn filter_ready_excludes_open_but_blocked_items() {
        let items = vec![
            make_item(1, Status::Open, Priority::P1, "a"),
            make_item(2, Status::Open, Priority::P1, "b"),
        ];
        // Item 2 is Open but explicitly blocked
        let blocked = [2_i64].into_iter().collect();
        let ready = filter_ready(items.into_iter(), &blocked);
        let ids: Vec<_> = ready.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![1]);
    }

    #[test]
    fn prime_summary_no_done_omits_recent_section() {
        let items = vec![
            make_item(1, Status::Open, Priority::P1, "open"),
        ];
        let summary = prime_summary(&items, &RigId::new("worker"));
        assert!(!summary.contains("Recent:"));
    }
}
