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
///
/// ```
/// use opengoose_board::RigId;
///
/// let rig = RigId::new("worker-01");
/// assert_eq!(rig.to_string(), "worker-01");
///
/// // 검증이 필요하면 try_new 사용
/// assert!(RigId::try_new("").is_err());
/// assert!(RigId::try_new("valid-id").is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RigId(pub String);

impl RigId {
    /// Create a RigId without validation.
    ///
    /// # Examples
    ///
    /// ```
    /// let id = opengoose_board::work_item::RigId::new("worker-1");
    /// assert_eq!(id.to_string(), "worker-1");
    /// ```
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Create a RigId with validation.
    ///
    /// # Examples
    ///
    /// ```
    /// use opengoose_board::work_item::RigId;
    /// assert!(RigId::try_new("valid-id").is_ok());
    /// assert!(RigId::try_new("").is_err());
    /// assert!(RigId::try_new("has/slash").is_err());
    /// ```
    pub fn try_new(id: impl Into<String>) -> Result<Self, String> {
        let s = id.into();
        if s.is_empty() {
            return Err("RigId cannot be empty".into());
        }
        if s.len() > 64 {
            return Err(format!("RigId exceeds 64 chars: {}", s.len()));
        }
        if s.contains("..") {
            return Err(format!("RigId cannot contain '..': {s}"));
        }
        if s.contains('/') {
            return Err(format!("RigId cannot contain '/': {s}"));
        }
        Ok(Self(s))
    }
}

impl AsRef<str> for RigId {
    fn as_ref(&self) -> &str {
        &self.0
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
///
/// ```
/// use opengoose_board::Status;
///
/// let status = Status::Open;
/// assert_eq!(status.as_str(), "Open");
/// assert_eq!(Status::parse("Done"), Some(Status::Done));
/// assert!(Status::Open.can_transition_to(Status::Claimed));
/// assert!(!Status::Done.can_transition_to(Status::Open));
/// ```
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, DeriveActiveEnum,
)]
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
    ///
    /// Merge precedence: higher value wins.
    ///
    /// # Examples
    ///
    /// ```
    /// use opengoose_board::work_item::Status;
    /// assert!(Status::Done.precedence() > Status::Open.precedence());
    /// ```
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
///
/// ```
/// use opengoose_board::Priority;
///
/// assert_eq!(Priority::default(), Priority::P1);
/// assert!(Priority::P0 > Priority::P1);
/// assert_eq!(Priority::parse("P0"), Some(Priority::P0));
/// ```
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    Serialize,
    Deserialize,
    EnumIter,
    DeriveActiveEnum,
)]
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
/// Phase 후반에 추가: project, parent, session_id, seq, assigned_to, notes, result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub created_by: RigId,
    pub created_at: DateTime<Utc>,
    pub status: Status,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub claimed_by: Option<RigId>,
    pub updated_at: DateTime<Utc>,
}

impl WorkItem {
    /// claimed_by 검증: 올바른 rig이 claim 중인지 확인.
    pub fn verify_claimed_by(&self, rig_id: &RigId) -> Result<(), BoardError> {
        match &self.claimed_by {
            Some(claimed) if claimed != rig_id => Err(BoardError::NotClaimedBy {
                id: self.id,
                claimed_by: claimed.clone(),
                attempted_by: rig_id.clone(),
            }),
            None => Err(BoardError::NotClaimed { id: self.id }),
            _ => Ok(()),
        }
    }
}

/// WorkItem 생성 요청 (Board.post 입력).
///
/// ```
/// use opengoose_board::{PostWorkItem, RigId, Priority};
///
/// let req = PostWorkItem {
///     title: "Implement feature X".into(),
///     description: "Add support for ...".into(),
///     created_by: RigId::new("dh"),
///     priority: Priority::P1,
///     tags: vec!["feature".into()],
/// };
/// assert_eq!(req.title, "Implement feature X");
/// ```
#[derive(Debug, Clone)]
pub struct PostWorkItem {
    pub title: String,
    pub description: String,
    pub created_by: RigId,
    pub priority: Priority,
    pub tags: Vec<String>,
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

    #[error("cannot remove system rig: {0}")]
    SystemRigProtected(String),

    #[error("database error: {0}")]
    DbError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rig_id_try_new_accepts_valid_ids() {
        assert!(RigId::try_new("worker-01").is_ok());
        assert!(RigId::try_new("dh").is_ok());
        assert!(RigId::try_new("researcher_v2").is_ok());
    }

    #[test]
    fn rig_id_try_new_rejects_empty() {
        let err = RigId::try_new("").unwrap_err();
        assert!(err.to_string().contains("empty"), "error: {err}");
    }

    #[test]
    fn rig_id_try_new_rejects_too_long() {
        let long = "a".repeat(65);
        let err = RigId::try_new(&long).unwrap_err();
        assert!(err.to_string().contains("64"), "error: {err}");
    }

    #[test]
    fn rig_id_try_new_rejects_path_traversal() {
        let err = RigId::try_new("../escape").unwrap_err();
        assert!(err.to_string().contains(".."), "error: {err}");
    }

    #[test]
    fn rig_id_roundtrip() {
        let rig = RigId::new("rig-1");
        assert_eq!(rig.to_string(), "rig-1");
        assert_eq!(rig.0, "rig-1");
    }

    #[test]
    fn project_ref_is_constructible() {
        let project = ProjectRef {
            name: "opengoose".into(),
            path: std::path::PathBuf::from("/tmp"),
        };
        assert_eq!(project.name, "opengoose");
        assert_eq!(project.path, std::path::PathBuf::from("/tmp"));
    }

    #[test]
    fn status_precedence_parse_display_and_ordering() {
        assert_eq!(Status::Open.as_str(), "Open");
        assert_eq!(Status::Open.precedence(), 0);
        assert_eq!(Status::Claimed.precedence(), 1);
        assert_eq!(Status::Stuck.precedence(), 2);
        assert_eq!(Status::Abandoned.precedence(), 3);
        assert_eq!(Status::Done.precedence(), 4);
        assert_eq!(Status::parse("Done"), Some(Status::Done));
        assert_eq!(Status::parse("Unknown"), None);
        assert_eq!(Status::Open.to_string(), "Open");
        assert_eq!(
            Status::Claimed.partial_cmp(&Status::Stuck),
            Some(std::cmp::Ordering::Less)
        );
        assert!(Status::Done > Status::Stuck);
    }

    #[test]
    fn priority_parse_and_urgency() {
        let p0 = Priority::parse("P0").expect("priority_parse_and_urgency should succeed");
        let p1 = Priority::parse("P1").expect("parse should succeed");
        let p2 = Priority::parse("P2").expect("parse should succeed");
        assert_eq!(Priority::default(), Priority::P1);
        assert_eq!(p0.urgency(), 2);
        assert_eq!(p1.urgency(), 1);
        assert_eq!(p2.urgency(), 0);
        assert!(p0 > p1);
        assert!(p1 > p2);
        assert_eq!(p1.as_str(), "P1");
    }

    #[test]
    fn work_item_verify_claimed_by() {
        let item = WorkItem {
            id: 1,
            title: "t".into(),
            description: "d".into(),
            created_by: RigId::new("creator"),
            created_at: Utc::now(),
            status: Status::Claimed,
            priority: Priority::P1,
            tags: vec![],
            claimed_by: Some(RigId::new("alice")),
            updated_at: Utc::now(),
        };

        item.verify_claimed_by(&RigId::new("alice"))
            .expect("alice claimed this item");
        match item.verify_claimed_by(&RigId::new("bob")) {
            Err(BoardError::NotClaimedBy {
                id,
                claimed_by,
                attempted_by,
            }) => {
                assert_eq!(id, 1);
                assert_eq!(claimed_by.0, "alice");
                assert_eq!(attempted_by.0, "bob");
            }
            other => panic!("expected NotClaimedBy, got {other:?}"),
        }
        let unclaimed = WorkItem {
            claimed_by: None,
            ..item
        };
        assert!(matches!(
            unclaimed.verify_claimed_by(&RigId::new("alice")),
            Err(BoardError::NotClaimed { id: 1 })
        ));
    }

    #[test]
    fn status_transition_rules() {
        assert!(Status::Open.can_transition_to(Status::Claimed));
        assert!(Status::Claimed.can_transition_to(Status::Done));
        assert!(Status::Claimed.can_transition_to(Status::Open));
        assert!(Status::Claimed.can_transition_to(Status::Stuck));
        assert!(Status::Stuck.can_transition_to(Status::Open));
        assert!(Status::Stuck.can_transition_to(Status::Abandoned));
        assert!(Status::Open.can_transition_to(Status::Abandoned));

        assert!(!Status::Done.can_transition_to(Status::Open));
        Status::Open
            .validate_transition(Status::Claimed)
            .expect("Open -> Claimed is valid");
        assert!(matches!(
            Status::Done.validate_transition(Status::Open),
            Err(TransitionError::Invalid { .. })
        ));
    }

    #[test]
    fn status_all_variants_as_str_and_parse() {
        for (status, s) in [
            (Status::Open, "Open"),
            (Status::Claimed, "Claimed"),
            (Status::Done, "Done"),
            (Status::Stuck, "Stuck"),
            (Status::Abandoned, "Abandoned"),
        ] {
            assert_eq!(status.as_str(), s);
            assert_eq!(Status::parse(s), Some(status));
        }
        assert_eq!(Status::parse("invalid"), None);
    }

    #[test]
    fn priority_all_variants_as_str_and_parse() {
        assert_eq!(Priority::P0.as_str(), "P0");
        assert_eq!(Priority::P2.as_str(), "P2");
        assert_eq!(Priority::parse("P2"), Some(Priority::P2));
        assert_eq!(Priority::parse("invalid"), None);
    }

    #[test]
    fn board_error_display() {
        let e = BoardError::NotFound(5);
        assert!(e.to_string().contains("5"));

        let e = BoardError::AlreadyClaimed {
            id: 2,
            claimed_by: RigId::new("x"),
        };
        assert!(e.to_string().contains("x"));

        let e = BoardError::NotClaimed { id: 3 };
        assert!(e.to_string().contains("3"));

        let e = BoardError::CyclicDependency(vec![1, 2]);
        assert!(e.to_string().contains("Cyclic") || e.to_string().contains("cyclic"));

        let e = BoardError::YearbookViolation {
            stamper: RigId::new("a"),
            target: RigId::new("a"),
        };
        assert!(e.to_string().contains("a"));

        let e = BoardError::InvalidScore(1.5);
        assert!(e.to_string().contains("1.5"));

        let e = BoardError::SystemRigProtected("human".to_string());
        assert!(e.to_string().contains("human"));

        let e = BoardError::BranchNotFound("main".to_string());
        assert!(e.to_string().contains("main"));

        let e = BoardError::MergeConflict("conflict on branch".to_string());
        assert!(e.to_string().contains("conflict"));

        let e = BoardError::DbError("connection failed".to_string());
        assert!(e.to_string().contains("connection failed"));

        let e = BoardError::NotClaimedBy {
            id: 4,
            claimed_by: RigId::new("alice"),
            attempted_by: RigId::new("bob"),
        };
        assert!(e.to_string().contains("alice"));
        assert!(e.to_string().contains("bob"));
    }
}
