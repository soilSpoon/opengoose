// Stamps + Trust — Wasteland 평판 시스템
//
// 다차원 평가: Quality, Reliability, Helpfulness
// 신뢰 사다리: L1 → L3 (가중 점수 기반 자동 승급)
// 졸업앨범 규칙: stamped_by != target_rig

use crate::work_item::{BoardError, RigId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 평가 차원.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dimension {
    Quality,
    Reliability,
    Helpfulness,
}

/// 작업 중요도 (가중치).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Leaf,   // 1.0x
    Branch, // 2.0x
    Root,   // 4.0x
}

impl Severity {
    pub fn weight(self) -> f32 {
        match self {
            Severity::Leaf => 1.0,
            Severity::Branch => 2.0,
            Severity::Root => 4.0,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Leaf => "Leaf",
            Severity::Branch => "Branch",
            Severity::Root => "Root",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Leaf" => Some(Severity::Leaf),
            "Branch" => Some(Severity::Branch),
            "Root" => Some(Severity::Root),
            _ => None,
        }
    }
}

/// 단일 stamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stamp {
    pub target_rig: RigId,
    pub work_item: i64,
    pub dimension: Dimension,
    pub score: f32, // -1.0 ~ +1.0
    pub severity: Severity,
    pub stamped_by: RigId,
    pub timestamp: DateTime<Utc>,
}

/// 신뢰 수준.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrustLevel {
    L1,   // Newcomer: < 3
    L1_5, // Recognized: >= 3
    L2,   // Contributor: >= 10
    L2_5, // Trusted: >= 25
    L3,   // Veteran: >= 50
}

impl TrustLevel {
    pub fn from_score(score: f32) -> Self {
        if score >= 50.0 {
            TrustLevel::L3
        } else if score >= 25.0 {
            TrustLevel::L2_5
        } else if score >= 10.0 {
            TrustLevel::L2
        } else if score >= 3.0 {
            TrustLevel::L1_5
        } else {
            TrustLevel::L1
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TrustLevel::L1 => "L1",
            TrustLevel::L1_5 => "L1.5",
            TrustLevel::L2 => "L2",
            TrustLevel::L2_5 => "L2.5",
            TrustLevel::L3 => "L3",
        }
    }
}

/// Stamp 저장소. Board에서 직접 관리.
#[derive(Debug, Clone, Default)]
pub struct StampStore {
    stamps: Vec<Stamp>,
}

impl StampStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stamp 추가. 졸업앨범 규칙 강제.
    pub fn add(&mut self, stamp: Stamp) -> Result<(), BoardError> {
        // 졸업앨범 규칙: stamped_by != target_rig
        if stamp.stamped_by == stamp.target_rig {
            return Err(BoardError::YearbookViolation {
                stamper: stamp.stamped_by,
                target: stamp.target_rig,
            });
        }

        // score 범위 검증
        if !(-1.0..=1.0).contains(&stamp.score) {
            return Err(BoardError::InvalidScore(stamp.score));
        }

        self.stamps.push(stamp);
        Ok(())
    }

    /// 특정 rig의 가중 점수 (시간 감쇠 적용).
    pub fn weighted_score(&self, rig_id: &RigId, now: DateTime<Utc>) -> f32 {
        self.stamps
            .iter()
            .filter(|s| s.target_rig == *rig_id)
            .map(|s| {
                let days = (now - s.timestamp).num_seconds() as f32 / 86400.0;
                let decay = 0.5_f32.powf(days / 30.0); // 30일 반감기
                s.severity.weight() * s.score * decay
            })
            .sum()
    }

    /// 특정 rig의 신뢰 수준.
    pub fn trust_level(&self, rig_id: &RigId, now: DateTime<Utc>) -> TrustLevel {
        TrustLevel::from_score(self.weighted_score(rig_id, now))
    }

    /// 제재 여부: 가중 점수 -5.0 이하.
    pub fn is_sanctioned(&self, rig_id: &RigId, now: DateTime<Utc>) -> bool {
        self.weighted_score(rig_id, now) < -5.0
    }

    /// 특정 rig의 모든 stamp.
    pub fn stamps_for(&self, rig_id: &RigId) -> Vec<&Stamp> {
        self.stamps.iter().filter(|s| s.target_rig == *rig_id).collect()
    }

    /// 전체 stamp 수.
    pub fn len(&self) -> usize {
        self.stamps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stamps.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stamp(target: &str, by: &str, score: f32, severity: Severity) -> Stamp {
        Stamp {
            target_rig: RigId::new(target),
            work_item: 1,
            dimension: Dimension::Quality,
            score,
            severity,
            stamped_by: RigId::new(by),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn yearbook_rule_enforced() {
        let mut store = StampStore::new();
        let stamp = make_stamp("alice", "alice", 1.0, Severity::Leaf);
        assert!(store.add(stamp).is_err());
    }

    #[test]
    fn weighted_score_no_decay() {
        let mut store = StampStore::new();
        store
            .add(make_stamp("dev", "reviewer", 0.8, Severity::Leaf))
            .unwrap();
        store
            .add(make_stamp("dev", "reviewer", 1.0, Severity::Root))
            .unwrap();

        let score = store.weighted_score(&RigId::new("dev"), Utc::now());
        // 0.8 * 1.0 + 1.0 * 4.0 ≈ 4.8 (tiny decay negligible)
        assert!((score - 4.8).abs() < 0.1);
    }

    #[test]
    fn trust_level_from_score() {
        assert_eq!(TrustLevel::from_score(0.0), TrustLevel::L1);
        assert_eq!(TrustLevel::from_score(3.0), TrustLevel::L1_5);
        assert_eq!(TrustLevel::from_score(10.0), TrustLevel::L2);
        assert_eq!(TrustLevel::from_score(25.0), TrustLevel::L2_5);
        assert_eq!(TrustLevel::from_score(50.0), TrustLevel::L3);
    }

    #[test]
    fn sanction_threshold() {
        let mut store = StampStore::new();
        store
            .add(make_stamp("bad", "reviewer", -1.0, Severity::Root))
            .unwrap(); // -4.0
        store
            .add(make_stamp("bad", "reviewer2", -1.0, Severity::Branch))
            .unwrap(); // -2.0
        // total ≈ -6.0

        assert!(store.is_sanctioned(&RigId::new("bad"), Utc::now()));
    }
}
