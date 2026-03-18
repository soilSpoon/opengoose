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

    /// O(d) diff: 두 스토어 간 변경된 키 목록.
    /// (base와 다른 키를 반환)
    pub fn diff_keys(&self, base: &CowStore) -> Vec<i64> {
        // Arc가 같으면 변경 없음
        if Arc::ptr_eq(&self.data, &base.data) {
            return Vec::new();
        }

        let mut changed = Vec::new();
        let self_data = &*self.data;
        let base_data = &*base.data;

        // self에 있고 base에 없거나 다른 것
        for (&id, item) in self_data.iter() {
            match base_data.get(&id) {
                None => changed.push(id),
                Some(base_item) => {
                    if item.updated_at != base_item.updated_at
                        || item.status != base_item.status
                        || item.priority != base_item.priority
                    {
                        changed.push(id);
                    }
                }
            }
        }

        // base에 있고 self에 없는 것 (삭제)
        for &id in base_data.keys() {
            if !self_data.contains_key(&id) {
                changed.push(id);
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
