// WorkItem — 보드의 기본 단위
//
// 모든 것이 작업 항목이다. 타입 분류 없음.
// worktree 생성, 블루프린트 적용 등은 rig가 실행 시점에 판단.
// ID는 Board가 중앙에서 AUTO INCREMENT로 생성.

use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── 식별자 타입 ──────────────────────────────────────────────

/// Rig 식별자. 사람도 rig이다.
/// 예: "dh" (사람), "researcher-01" (AI)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RigId(pub String);

impl RigId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for RigId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 프로젝트 참조. "코드 작업할 때 어디서 하는지".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRef {
    pub name: String,
    pub path: PathBuf,
}

// ── Status enum ──────────────────────────────────────────────

/// 작업 항목 상태.
/// 순서: Done > Abandoned > Stuck > Claimed > Open (머지 시 더 진행된 쪽이 이김)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Text")]
pub enum Status {
    #[sea_orm(string_value = "Open")]
    Open,
    #[sea_orm(string_value = "Claimed")]
    Claimed,
    #[sea_orm(string_value = "Done")]
    Done,
    #[sea_orm(string_value = "Stuck")]
    Stuck,
    #[sea_orm(string_value = "Abandoned")]
    Abandoned,
}

impl Status {
    /// 머지 시 사용: 더 진행된 상태가 이긴다.
    pub fn precedence(self) -> u8 {
        match self {
            Status::Open => 0,
            Status::Claimed => 1,
            Status::Stuck => 2,
            Status::Abandoned => 3,
            Status::Done => 4,
        }
    }
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "Open",
            Status::Claimed => "Claimed",
            Status::Done => "Done",
            Status::Stuck => "Stuck",
            Status::Abandoned => "Abandoned",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Open" => Some(Status::Open),
            "Claimed" => Some(Status::Claimed),
            "Done" => Some(Status::Done),
            "Stuck" => Some(Status::Stuck),
            "Abandoned" => Some(Status::Abandoned),
            _ => None,
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl PartialOrd for Status {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Status {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.precedence().cmp(&other.precedence())
    }
}

// ── Priority enum ────────────────────────────────────────────

/// 우선순위.
/// 순서: P0 > P1 > P2 (에스컬레이션만, 내려가지 않음)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Text")]
pub enum Priority {
    #[sea_orm(string_value = "P0")]
    P0,
    #[default]
    #[sea_orm(string_value = "P1")]
    P1,
    #[sea_orm(string_value = "P2")]
    P2,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Priority::P0 => "P0",
            Priority::P1 => "P1",
            Priority::P2 => "P2",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "P0" => Some(Priority::P0),
            "P1" => Some(Priority::P1),
            "P2" => Some(Priority::P2),
            _ => None,
        }
    }

    pub fn urgency(self) -> u8 {
        match self {
            Priority::P0 => 2,
            Priority::P1 => 1,
            Priority::P2 => 0,
        }
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.urgency().cmp(&other.urgency())
    }
}


// ── WorkItem ─────────────────────────────────────────────────

/// 보드의 기본 단위. 모든 것이 작업 항목이다.
///
/// Phase 2~3에서 추가: project, parent, session_id, seq, assigned_to, notes, tags, result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub created_by: RigId,
    pub created_at: DateTime<Utc>,
    pub status: Status,
    pub priority: Priority,
    pub claimed_by: Option<RigId>,
    pub updated_at: DateTime<Utc>,
}

/// WorkItem 생성 요청 (Board.post 입력)
#[derive(Debug, Clone)]
pub struct PostWorkItem {
    pub title: String,
    pub description: String,
    pub created_by: RigId,
    pub priority: Priority,
}

// ── 상태 전이 ────────────────────────────────────────────────

/// 허용되는 상태 전이.
///
/// Open → Claimed        rig가 claim
/// Claimed → Done        rig가 완료
/// Claimed → Open        unclaim, crash 복구, timeout
/// Claimed → Stuck       CI 2라운드 초과, stuck timeout
/// Stuck → Open          /retry
/// Stuck → Abandoned     /abandon
/// Open → Abandoned      /abandon
#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("invalid transition: {from:?} → {to:?}")]
    Invalid { from: Status, to: Status },
}

impl Status {
    /// 상태 전이가 유효한지 검증.
    pub fn can_transition_to(self, to: Status) -> bool {
        matches!(
            (self, to),
            (Status::Open, Status::Claimed)
                | (Status::Claimed, Status::Done)
                | (Status::Claimed, Status::Open)
                | (Status::Claimed, Status::Stuck)
                | (Status::Stuck, Status::Open)
                | (Status::Stuck, Status::Abandoned)
                | (Status::Open, Status::Abandoned)
        )
    }

    pub fn validate_transition(self, to: Status) -> Result<(), TransitionError> {
        if self.can_transition_to(to) {
            Ok(())
        } else {
            Err(TransitionError::Invalid { from: self, to })
        }
    }
}

// ── 에러 타입 ────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum BoardError {
    #[error("work item not found: {0}")]
    NotFound(i64),

    #[error("work item {id} already claimed by {claimed_by}")]
    AlreadyClaimed { id: i64, claimed_by: RigId },

    #[error("work item {id} is not claimed")]
    NotClaimed { id: i64 },

    #[error("work item {id} claimed by {claimed_by}, not {attempted_by}")]
    NotClaimedBy {
        id: i64,
        claimed_by: RigId,
        attempted_by: RigId,
    },

    #[error(transparent)]
    InvalidTransition(#[from] TransitionError),

    #[error("branch not found: {0}")]
    BranchNotFound(String),

    #[error("merge conflict: {0}")]
    MergeConflict(String),

    #[error("cyclic dependency detected: {0:?}")]
    CyclicDependency(Vec<i64>),

    #[error("yearbook rule violation: stamped_by ({stamper}) == target_rig ({target})")]
    YearbookViolation { stamper: RigId, target: RigId },

    #[error("stamp score out of range: {0} (must be -1.0 ~ +1.0)")]
    InvalidScore(f32),

    #[error("database error: {0}")]
    DbError(String),
}
