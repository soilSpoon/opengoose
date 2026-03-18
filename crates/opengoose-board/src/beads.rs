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
        .filter(|item| {
            item.status == Status::Open && !blocked_ids.contains(&item.id)
        })
        .collect();

    ready.sort_by(|a, b| b.priority.urgency().cmp(&a.priority.urgency()));
    ready
}

/// prime() — 에이전트 컨텍스트 요약. Phase 1: 최소 구현.
pub fn prime_summary(items: &[WorkItem], rig_id: &RigId) -> String {
    let open = items.iter().filter(|i| i.status == Status::Open).count();
    let claimed = items.iter().filter(|i| i.status == Status::Claimed).count();
    let done = items.iter().filter(|i| i.status == Status::Done).count();

    let recent_done: Vec<_> = items
        .iter()
        .filter(|i| i.status == Status::Done)
        .take(3)
        .collect();

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
