use crate::branch::Branch;
use crate::merge::{MergeResult, MergedItem, merge_work_item};
use crate::work_item::{BoardError, WorkItem};

use super::CowStore;

/// Resolve one item during 3-way merge. Returns None if no change needed.
fn resolve_merge_item(
    id: i64,
    branch_item: &WorkItem,
    base_item: Option<&WorkItem>,
    main_item: Option<&WorkItem>,
) -> Option<MergedItem> {
    match (base_item, main_item) {
        // 양쪽 존재: 3-way merge
        (Some(base), Some(current_main)) => {
            let branch_changed = branch_item != base;
            let main_changed = current_main != base;
            match (branch_changed, main_changed) {
                (false, _) => None,
                (true, false) => Some(MergedItem {
                    item_id: id,
                    item: branch_item.clone(),
                    convergences: vec![],
                }),
                (true, true) => Some(merge_work_item(base, branch_item, current_main)),
            }
        }
        // 브랜치에서 신규 생성
        (None, None) => Some(MergedItem {
            item_id: id,
            item: branch_item.clone(),
            convergences: vec![],
        }),
        // base 없이 main에만 존재 → 브랜치를 base로 사용
        (None, Some(current_main)) => {
            Some(merge_work_item(branch_item, branch_item, current_main))
        }
        // base에 있었지만 main에서 삭제됨 → 무시
        (Some(_), None) => None,
    }
}

impl CowStore {
    /// 3-way merge: base (branch creation snapshot) vs branch vs current main.
    pub fn merge(&mut self, branch: Branch) -> Result<MergeResult, BoardError> {
        let branch_name = branch.name.clone();

        let merged_items = {
            let main = std::sync::Arc::make_mut(&mut self.main);

            // 변경/추가된 아이템 머지
            let merged: Vec<_> = branch
                .data
                .iter()
                .filter_map(|(id, branch_item)| {
                    let result = resolve_merge_item(
                        *id,
                        branch_item,
                        branch.base_data.get(id),
                        main.get(id),
                    )?;
                    main.insert(*id, result.item.clone());
                    Some(result)
                })
                .collect();

            // 브랜치에서 삭제된 아이템 제거
            branch
                .base_data
                .keys()
                .filter(|id| !branch.data.contains_key(id))
                .for_each(|id| {
                    main.remove(id);
                });

            merged
        };

        let commit = self.append_commit(
            &branch_name,
            format!("merge {branch_name} (base_commit: {})", branch.base_commit),
        );

        Ok(MergeResult {
            merged_items,
            commit_id: commit.id.0,
        })
    }
}
