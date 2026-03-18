// opengoose-board — Wanted Board + Beads + CoW Store
//
// 이 크레이트는 LLM, 세션, 플랫폼에 대해 아무것도 모른다.
// 순수한 데이터 레이어: 작업 항목, 브랜치, 머지, 신뢰.

pub mod entity;
pub mod db_board;
pub mod beads;
pub mod board;
pub mod branch;
pub mod merge;
pub mod relations;
pub mod stamps;
pub mod store;
pub mod work_item;

// Re-exports: 가장 자주 사용하는 타입들
pub use board::Board;
pub use stamps::{Stamp, StampStore, TrustLevel};
pub use store::CowStore;
pub use work_item::{BoardError, PostWorkItem, Priority, RigId, Status, WorkItem};
