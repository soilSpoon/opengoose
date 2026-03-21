// opengoose-board — Wanted Board + Beads
//
// 이 크레이트는 LLM, 세션, 플랫폼에 대해 아무것도 모른다.
// 순수한 데이터 레이어: 작업 항목, 관계, 신뢰.

pub mod beads;
pub mod board;
pub mod entity;
pub mod relations;
pub mod rigs;
pub mod stamp_ops;
pub mod stamps;
pub mod work_item;
pub mod work_items;

// Re-exports: 가장 자주 사용하는 타입들
pub use board::{AddStampParams, Board};
pub use stamps::TrustLevel;
pub use work_item::{BoardError, PostWorkItem, Priority, RigId, Status, WorkItem};
