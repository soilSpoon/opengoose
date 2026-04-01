//! Wanted Board — 작업 게시판 + CowStore(인메모리 CoW 저장소) + Stamp/Bead 관리.
//! 에이전트가 Board에서 작업을 가져가고(claim), 완료하면 제출(submit)한다.

pub mod beads;
pub mod board;
pub mod branch;
pub mod entity;
pub mod memory;
pub mod merge;
pub mod relations;
pub mod rigs;
pub mod stamp_ops;
pub mod stamps;
pub mod store;
pub mod work_item;
pub mod work_items;

#[cfg(test)]
pub mod test_fixtures;
#[cfg(test)]
pub mod test_helpers;

// Re-exports: 가장 자주 사용하는 타입들
pub use board::{AddStampParams, Board};
pub use branch::Branch;
pub use memory::{Memory, MemoryScope};
pub use merge::{LwwField, MergeResult, MergeStrategy, Mergeable};
pub use stamps::{DimensionScores, TrustLevel};
pub use work_item::{BoardError, PostWorkItem, Priority, RigId, Status, WorkItem};
