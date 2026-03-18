// CoW Store — Dolt 영감의 Copy-on-Write BTreeMap
//
// Arc<BTreeMap>으로 O(1) 브랜칭, SHA-256 루트 해시.
// prollytree 크레이트는 사용하지 않는다 (v1에서 문제 발생).

use crate::work_item::WorkItem;
use std::collections::BTreeMap;
use std::sync::Arc;

/// 콘텐츠 주소 지정이 가능한 Copy-on-Write BTreeMap.
///
/// - O(1) 브랜칭 (Arc clone)
/// - SHA-256 루트 해시 (캐시됨, 변이 시 무효화)
/// - O(d) diff (변경된 키만 비교)
#[derive(Debug, Clone)]
pub struct CowStore {
    data: Arc<BTreeMap<i64, WorkItem>>,
    /// SHA-256 루트 해시 캐시. None = 무효화됨 (쓰기 후).
    root_hash: Option<[u8; 32]>,
}

impl CowStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(BTreeMap::new()),
            root_hash: None,
        }
    }

    /// O(1) 스냅샷 (브랜칭용). Arc clone만.
    pub fn snapshot(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            root_hash: self.root_hash,
        }
    }

    /// 읽기: 키로 작업 항목 조회.
    pub fn get(&self, id: i64) -> Option<&WorkItem> {
        self.data.get(&id)
    }

    /// 읽기: 모든 항목 반복.
    pub fn iter(&self) -> impl Iterator<Item = (&i64, &WorkItem)> {
        self.data.iter()
    }

    /// 읽기: 항목 수.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 쓰기: 항목 삽입/갱신. CoW 발생 (첫 쓰기 시 BTreeMap clone).
    pub fn insert(&mut self, id: i64, item: WorkItem) {
        Arc::make_mut(&mut self.data).insert(id, item);
        self.root_hash = None; // 캐시 무효화
    }

    /// 쓰기: 항목 제거.
    pub fn remove(&mut self, id: i64) -> Option<WorkItem> {
        let removed = Arc::make_mut(&mut self.data).remove(&id);
        if removed.is_some() {
            self.root_hash = None;
        }
        removed
    }

    /// 가변 참조로 항목 수정. CoW 발생.
    pub fn get_mut(&mut self, id: i64) -> Option<&mut WorkItem> {
        self.root_hash = None;
        Arc::make_mut(&mut self.data).get_mut(&id)
    }

    /// get_mut + NotFound 에러. Board 메서드에서 단일 조회로 검증+변경 가능.
    pub fn get_mut_or(&mut self, id: i64) -> Result<&mut WorkItem, crate::work_item::BoardError> {
        self.get_mut(id).ok_or(crate::work_item::BoardError::NotFound(id))
    }

    /// 모든 키 목록.
    pub fn keys(&self) -> impl Iterator<Item = &i64> {
        self.data.keys()
    }

    /// 모든 값 목록.
    pub fn values(&self) -> impl Iterator<Item = &WorkItem> {
        self.data.values()
    }

    /// SHA-256 루트 해시 계산 (또는 캐시 반환).
    pub fn root_hash(&mut self) -> [u8; 32] {
        if let Some(hash) = self.root_hash {
            return hash;
        }
        let hash = self.compute_hash();
        self.root_hash = Some(hash);
        hash
    }

    /// O(n+m) diff: 두 스토어 간 변경된 키 목록.
    /// BTreeMap 정렬 특성을 활용한 merge-walk — 조회 없이 한 번에 비교.
    pub fn diff_keys(&self, base: &CowStore) -> Vec<i64> {
        if Arc::ptr_eq(&self.data, &base.data) {
            return Vec::new();
        }

        let mut changed = Vec::new();
        let mut self_iter = self.data.iter().peekable();
        let mut base_iter = base.data.iter().peekable();

        loop {
            match (self_iter.peek(), base_iter.peek()) {
                (Some(&(sid, s_item)), Some(&(bid, b_item))) => match sid.cmp(bid) {
                    std::cmp::Ordering::Equal => {
                        if s_item.updated_at != b_item.updated_at
                            || s_item.status != b_item.status
                            || s_item.priority != b_item.priority
                        {
                            changed.push(*sid);
                        }
                        self_iter.next();
                        base_iter.next();
                    }
                    std::cmp::Ordering::Less => {
                        changed.push(*sid); // self에만 있음 (추가)
                        self_iter.next();
                    }
                    std::cmp::Ordering::Greater => {
                        changed.push(*bid); // base에만 있음 (삭제)
                        base_iter.next();
                    }
                },
                (Some(&(sid, _)), None) => {
                    changed.push(*sid);
                    self_iter.next();
                }
                (None, Some(&(bid, _))) => {
                    changed.push(*bid);
                    base_iter.next();
                }
                (None, None) => break,
            }
        }

        changed
    }

    fn compute_hash(&self) -> [u8; 32] {
        use std::hash::{Hash, Hasher};

        // 간단한 해시: 모든 항목의 (id, updated_at, status)를 직렬화하여 SHA-256
        // 실제 SHA-256은 sha2 크레이트 없이 간단한 대안 사용
        // Phase 1에서는 deterministic hash면 충분
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for (&id, item) in self.data.iter() {
            id.hash(&mut hasher);
            item.status.hash(&mut hasher);
            item.priority.hash(&mut hasher);
            item.updated_at.timestamp_nanos_opt().hash(&mut hasher);
        }
        let h = hasher.finish();
        let mut result = [0u8; 32];
        result[..8].copy_from_slice(&h.to_le_bytes());
        // 나머지는 0으로 — Phase 4에서 SHA-256으로 교체
        result
    }
}

impl Default for CowStore {
    fn default() -> Self {
        Self::new()
    }
}

// Status와 Priority에 Hash derive 추가 필요 — work_item.rs에서 처리

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
            tags: vec![],
            claimed_by: None,
            updated_at: now,
        }
    }

    #[test]
    fn diff_keys_after_remove() {
        let mut store = CowStore::new();
        store.insert(1, make_item(1));
        store.insert(2, make_item(2));

        let base = store.snapshot();
        store.remove(1);

        let diff = store.diff_keys(&base);
        assert!(diff.contains(&1));
        assert!(!diff.contains(&2));
    }

    #[test]
    fn diff_keys_identical_arc_is_empty() {
        let mut store = CowStore::new();
        store.insert(1, make_item(1));

        let snapshot = store.snapshot();
        assert!(store.diff_keys(&snapshot).is_empty());
    }

    #[test]
    fn snapshot_is_isolated() {
        let mut store = CowStore::new();
        store.insert(1, make_item(1));

        let snapshot = store.snapshot();
        store.insert(2, make_item(2));

        assert_eq!(store.len(), 2);
        assert_eq!(snapshot.len(), 1);
    }

    #[test]
    fn root_hash_invalidated_on_write() {
        let mut store = CowStore::new();
        store.insert(1, make_item(1));
        let h1 = store.root_hash();

        store.insert(2, make_item(2));
        let h2 = store.root_hash();

        assert_ne!(h1, h2);
    }

    #[test]
    fn get_mut_or_not_found() {
        let mut store = CowStore::new();
        assert!(store.get_mut_or(999).is_err());
    }
}
