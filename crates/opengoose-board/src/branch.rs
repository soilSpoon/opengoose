// Branch — 에이전트별 데이터 격리
//
// branch() = Arc clone (O(1))
// merge() = 3-way (base vs source vs dest)
// drop() = main은 영향 없음

use crate::store::CowStore;

/// 커밋 로그 항목 (감사 추적용 append-only 해시 체인).
#[derive(Debug, Clone)]
pub struct CommitEntry {
    pub branch: String,
    pub message: String,
    pub root_hash: [u8; 32],
    pub parent_hash: Option<[u8; 32]>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 브랜치 = 이름 + base 스냅샷 + 현재 스토어.
/// 순수 스냅샷 모델: 분기 시점의 데이터만 본다.
#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    /// 분기 시점의 스냅샷 (3-way merge의 base).
    pub base: CowStore,
    /// 현재 작업 중인 스토어 (CoW로 변경 추적).
    pub store: CowStore,
}

impl Branch {
    /// main에서 브랜치 생성. O(1) — Arc clone.
    pub fn from_main(name: impl Into<String>, main: &CowStore) -> Self {
        Self {
            name: name.into(),
            base: main.snapshot(),
            store: main.snapshot(),
        }
    }

    /// 브랜치에서 변경된 키 목록 (base 대비).
    pub fn changed_keys(&self) -> Vec<i64> {
        self.store.diff_keys(&self.base)
    }
}
