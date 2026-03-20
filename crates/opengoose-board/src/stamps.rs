// Stamps + Trust — Wasteland 평판 시스템
//
// 다차원 평가: Quality, Reliability, Helpfulness
// 신뢰 사다리: L1 → L3 (가중 점수 기반 자동 승급)
// 졸업앨범 규칙: stamped_by != target_rig

use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_level_from_score() {
        assert_eq!(TrustLevel::from_score(0.0), TrustLevel::L1);
        assert_eq!(TrustLevel::from_score(3.0), TrustLevel::L1_5);
        assert_eq!(TrustLevel::from_score(10.0), TrustLevel::L2);
        assert_eq!(TrustLevel::from_score(25.0), TrustLevel::L2_5);
        assert_eq!(TrustLevel::from_score(50.0), TrustLevel::L3);
    }

    #[test]
    fn severity_as_str_all_variants() {
        assert_eq!(Severity::Leaf.as_str(), "Leaf");
        assert_eq!(Severity::Branch.as_str(), "Branch");
        assert_eq!(Severity::Root.as_str(), "Root");
    }

    #[test]
    fn severity_parse_returns_none_for_unknown() {
        assert!(Severity::parse("Unknown").is_none());
        assert!(Severity::parse("").is_none());
        assert_eq!(Severity::parse("Branch"), Some(Severity::Branch));
        assert_eq!(Severity::parse("Root"), Some(Severity::Root));
    }

    #[test]
    fn trust_level_as_str_all_variants() {
        assert_eq!(TrustLevel::L1.as_str(), "L1");
        assert_eq!(TrustLevel::L1_5.as_str(), "L1.5");
        assert_eq!(TrustLevel::L2.as_str(), "L2");
        assert_eq!(TrustLevel::L2_5.as_str(), "L2.5");
        assert_eq!(TrustLevel::L3.as_str(), "L3");
    }

    #[test]
    fn severity_weight_all_variants() {
        assert_eq!(Severity::Leaf.weight(), 1.0);
        assert_eq!(Severity::Branch.weight(), 2.0);
        assert_eq!(Severity::Root.weight(), 4.0);
    }
}
