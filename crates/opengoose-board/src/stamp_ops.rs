// Stamp operations for Board.

use crate::board::{AddStampParams, Board, db_err, stamp_weighted_value};
use crate::entity;
use crate::stamps::{Severity, TrustLevel};
use crate::work_item::{BoardError, RigId};
use chrono::Utc;
use sea_orm::*;

impl Board {
    pub async fn add_stamp(&self, p: AddStampParams<'_>) -> Result<i64, BoardError> {
        // 졸업앨범 규칙
        if p.stamped_by == p.target_rig {
            return Err(BoardError::YearbookViolation {
                stamper: crate::work_item::RigId::new(p.stamped_by),
                target: crate::work_item::RigId::new(p.target_rig),
            });
        }
        // score 범위
        if !(-1.0..=1.0).contains(&p.score) {
            return Err(BoardError::InvalidScore(p.score));
        }
        // severity 검증
        let sev = Severity::parse(p.severity).ok_or_else(|| {
            BoardError::DbError(format!(
                "invalid severity: {:?} (expected Leaf, Branch, or Root)",
                p.severity
            ))
        })?;

        let result = entity::stamp::Entity::insert(entity::stamp::ActiveModel {
            id: NotSet,
            target_rig: Set(p.target_rig.to_string()),
            work_item_id: Set(p.work_item_id),
            dimension: Set(p.dimension.to_string()),
            score: Set(p.score),
            severity: Set(sev.as_str().to_string()),
            stamped_by: Set(p.stamped_by.to_string()),
            comment: Set(p.comment.map(|s| s.to_string())),
            evolved_at: NotSet,
            active_skill_versions: Set(p.active_skill_versions.map(|s| s.to_string())),
            timestamp: Set(chrono::Utc::now()),
        })
        .exec(&self.db)
        .await
        .map_err(db_err)?;

        self.stamp_notify.notify_waiters();
        Ok(result.last_insert_id)
    }

    /// 특정 work_item에 대한 모든 stamp 조회.
    pub async fn stamps_for_item(
        &self,
        work_item_id: i64,
    ) -> Result<Vec<entity::stamp::Model>, BoardError> {
        entity::stamp::Entity::find()
            .filter(entity::stamp::Column::WorkItemId.eq(work_item_id))
            .all(&self.db)
            .await
            .map_err(db_err)
    }

    /// 특정 rig의 모든 stamp 조회.
    pub async fn stamps_for_rig(
        &self,
        rig_id: &RigId,
    ) -> Result<Vec<entity::stamp::Model>, BoardError> {
        entity::stamp::Entity::find()
            .filter(entity::stamp::Column::TargetRig.eq(rig_id.as_ref()))
            .all(&self.db)
            .await
            .map_err(db_err)
    }

    /// 가중 점수 (시간 감쇠 적용). 30일 반감기.
    pub async fn weighted_score(&self, rig_id: &RigId) -> Result<f32, BoardError> {
        let stamps = entity::stamp::Entity::find()
            .filter(entity::stamp::Column::TargetRig.eq(rig_id.as_ref()))
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let now = Utc::now();
        Ok(stamps.iter().map(|s| stamp_weighted_value(s, now)).sum())
    }

    /// 신뢰 수준. stamps.rs의 TrustLevel::from_score() 재사용.
    pub async fn trust_level(&self, rig_id: &RigId) -> Result<&'static str, BoardError> {
        let score = self.weighted_score(rig_id).await?;
        Ok(TrustLevel::from_score(score).as_str())
    }

    /// 특정 rig의 stamps + 차원별/전체 가중 점수를 한 번에 조회.
    pub async fn stamps_with_scores(
        &self,
        rig_id: &RigId,
    ) -> Result<
        (
            Vec<entity::stamp::Model>,
            crate::stamps::DimensionScores,
            f32,
        ),
        BoardError,
    > {
        let stamps = entity::stamp::Entity::find()
            .filter(entity::stamp::Column::TargetRig.eq(rig_id.as_ref()))
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let now = Utc::now();
        let (dimensions, total) = stamps.iter().fold(
            (crate::stamps::DimensionScores::default(), 0.0_f32),
            |(mut dims, total), s| {
                let weighted = stamp_weighted_value(s, now);
                dims.accumulate(&s.dimension, weighted);
                (dims, total + weighted)
            },
        );

        Ok((stamps, dimensions, total))
    }

    /// 모든 rig의 가중 점수를 배치 조회. N+1 쿼리 방지.
    pub async fn batch_rig_scores(
        &self,
    ) -> Result<std::collections::HashMap<String, (f32, &'static str)>, BoardError> {
        let stamps = entity::stamp::Entity::find()
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let now = Utc::now();
        let scores = stamps.iter().fold(
            std::collections::HashMap::<String, f32>::new(),
            |mut acc, s| {
                *acc.entry(s.target_rig.clone()).or_default() += stamp_weighted_value(s, now);
                acc
            },
        );

        Ok(scores
            .into_iter()
            .map(|(id, score)| (id, (score, TrustLevel::from_score(score).as_str())))
            .collect())
    }

    pub async fn unprocessed_low_stamps(
        &self,
        threshold: f32,
    ) -> Result<Vec<entity::stamp::Model>, BoardError> {
        entity::stamp::Entity::find()
            .filter(entity::stamp::Column::Score.lt(threshold))
            .filter(entity::stamp::Column::EvolvedAt.is_null())
            .all(&self.db)
            .await
            .map_err(db_err)
    }

    /// Get low stamps from the last N days (for sweep mode).
    pub async fn recent_low_stamps(
        &self,
        threshold: f32,
        days: i64,
    ) -> Result<Vec<entity::stamp::Model>, BoardError> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        entity::stamp::Entity::find()
            .filter(entity::stamp::Column::Score.lt(threshold))
            .filter(entity::stamp::Column::Timestamp.gt(cutoff))
            .order_by_desc(entity::stamp::Column::Timestamp)
            .limit(50)
            .all(&self.db)
            .await
            .map_err(db_err)
    }

    pub async fn mark_stamp_evolved(&self, stamp_id: i64) -> Result<bool, BoardError> {
        use sea_orm::sea_query::Expr;
        let result = entity::stamp::Entity::update_many()
            .col_expr(
                entity::stamp::Column::EvolvedAt,
                Expr::value(chrono::Utc::now()),
            )
            .filter(entity::stamp::Column::Id.eq(stamp_id))
            .filter(entity::stamp::Column::EvolvedAt.is_null())
            .exec(&self.db)
            .await
            .map_err(db_err)?;
        Ok(result.rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{new_board, post_req, stamp_params};
    use crate::work_item::RigId;

    // ── add_stamp ────────────────────────────────────────────

    #[tokio::test]
    async fn stamp_yearbook_rule_enforced_db() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");
        let result = board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.5, "Leaf", "rig-a",
            ))
            .await;
        assert!(
            matches!(result, Err(BoardError::YearbookViolation { .. })),
            "expected YearbookViolation, got {result:?}"
        );
    }

    #[tokio::test]
    async fn stamp_invalid_score_rejected_db() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");
        let result = board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 1.5, "Leaf", "rig-b",
            ))
            .await;
        assert!(
            matches!(result, Err(BoardError::InvalidScore(s)) if (s - 1.5).abs() < f32::EPSILON),
            "expected InvalidScore(1.5), got {result:?}"
        );
    }

    #[tokio::test]
    async fn stamp_negative_score_out_of_range_rejected() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");
        let result = board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", -1.5, "Leaf", "rig-b",
            ))
            .await;
        assert!(
            matches!(result, Err(BoardError::InvalidScore(s)) if (s - (-1.5)).abs() < f32::EPSILON),
            "expected InvalidScore(-1.5), got {result:?}"
        );
    }

    #[tokio::test]
    async fn stamp_invalid_severity_rejected_db() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");
        let result = board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.5, "Invalid", "rig-b",
            ))
            .await;
        assert!(
            matches!(result, Err(BoardError::DbError(_))),
            "expected DbError for invalid severity, got {result:?}"
        );
    }

    #[tokio::test]
    async fn stamp_custom_dimension_accepted() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Creativity",
                0.5,
                "Leaf",
                "rig-b",
            ))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn add_stamp_happy_path_returns_id() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");
        let id = board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.8, "Leaf", "rig-b",
            ))
            .await
            .expect("add_stamp should succeed");
        assert!(id > 0, "stamp id should be positive, got {id}");
    }

    #[tokio::test]
    async fn add_stamp_boundary_scores_accepted() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        // Exact boundaries: -1.0 and 1.0 should be accepted
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", -1.0, "Leaf", "rig-b",
            ))
            .await
            .expect("score -1.0 should be accepted");
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 1.0, "Leaf", "rig-b",
            ))
            .await
            .expect("score 1.0 should be accepted");
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.0, "Leaf", "rig-b",
            ))
            .await
            .expect("score 0.0 should be accepted");
    }

    #[tokio::test]
    async fn add_stamp_all_severities_accepted() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        for severity in ["Leaf", "Branch", "Root"] {
            board
                .add_stamp(stamp_params(
                    "rig-a", item.id, "Quality", 0.5, severity, "rig-b",
                ))
                .await
                .unwrap_or_else(|_| panic!("severity {severity} should be accepted"));
        }
    }

    // ── stamps_for_item ──────────────────────────────────────

    #[tokio::test]
    async fn stamps_for_item_returns_matching_stamps() {
        let board = new_board().await;
        let item1 = board
            .post(post_req("task-1"))
            .await
            .expect("board post should succeed");
        let item2 = board
            .post(post_req("task-2"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item1.id, "Quality", 0.5, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item1.id,
                "Reliability",
                0.7,
                "Branch",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-b", item2.id, "Quality", 0.3, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let stamps = board
            .stamps_for_item(item1.id)
            .await
            .expect("stamps_for_item should succeed");
        assert_eq!(stamps.len(), 2);
        assert!(stamps.iter().all(|s| s.work_item_id == item1.id));

        let stamps2 = board
            .stamps_for_item(item2.id)
            .await
            .expect("stamps_for_item should succeed");
        assert_eq!(stamps2.len(), 1);
    }

    #[tokio::test]
    async fn stamps_for_item_empty_when_no_stamps() {
        let board = new_board().await;
        let stamps = board
            .stamps_for_item(9999)
            .await
            .expect("stamps_for_item should succeed for nonexistent item");
        assert!(stamps.is_empty());
    }

    // ── stamps_for_rig ───────────────────────────────────────

    #[tokio::test]
    async fn stamps_for_rig_returns_all_for_rig() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-x", item.id, "Quality", 0.5, "Leaf", "human",
            ))
            .await
            .expect("board operation should succeed");
        board
            .add_stamp(stamp_params(
                "rig-x",
                item.id,
                "Reliability",
                0.8,
                "Leaf",
                "human",
            ))
            .await
            .expect("board operation should succeed");
        board
            .add_stamp(stamp_params(
                "rig-y", item.id, "Quality", 0.3, "Leaf", "human",
            ))
            .await
            .expect("board operation should succeed");

        let stamps = board
            .stamps_for_rig(&RigId::new("rig-x"))
            .await
            .expect("async operation should succeed");
        assert_eq!(stamps.len(), 2);

        let stamps_y = board
            .stamps_for_rig(&RigId::new("rig-y"))
            .await
            .expect("async operation should succeed");
        assert_eq!(stamps_y.len(), 1);

        let empty = board
            .stamps_for_rig(&RigId::new("nobody"))
            .await
            .expect("async operation should succeed");
        assert!(empty.is_empty());
    }

    // ── weighted_score ───────────────────────────────────────

    #[tokio::test]
    async fn weighted_score_zero_for_unknown_rig() {
        let board = new_board().await;
        let score = board
            .weighted_score(&RigId::new("nonexistent"))
            .await
            .expect("weighted_score should succeed");
        assert_eq!(score, 0.0);
    }

    #[tokio::test]
    async fn weighted_score_positive_after_stamps() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.8, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Reliability",
                0.5,
                "Branch",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let score = board
            .weighted_score(&RigId::new("rig-a"))
            .await
            .expect("weighted_score should succeed");
        // Fresh stamps: Leaf(1.0)*0.8 + Branch(2.0)*0.5 = 0.8 + 1.0 = 1.8
        // With near-zero time decay the score should be close to 1.8
        assert!(score > 1.5, "expected score > 1.5, got {score}");
        assert!(score <= 1.8, "expected score <= 1.8, got {score}");
    }

    #[tokio::test]
    async fn weighted_score_respects_negative_stamps() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", -0.5, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let score = board
            .weighted_score(&RigId::new("rig-a"))
            .await
            .expect("weighted_score should succeed");
        assert!(score < 0.0, "expected negative score, got {score}");
    }

    // ── trust_level ──────────────────────────────────────────

    #[tokio::test]
    async fn trust_level_l1_for_unknown_rig() {
        let board = new_board().await;
        let level = board
            .trust_level(&RigId::new("ghost"))
            .await
            .expect("trust_level should succeed");
        assert_eq!(level, "L1");
    }

    #[tokio::test]
    async fn trust_level_reflects_accumulated_score() {
        let board = new_board().await;
        // Add enough stamps to push past L1 (need score >= 3.0)
        // Root severity = 4.0 weight. Score 1.0 => weighted ~4.0 per stamp
        for i in 0..3 {
            let item = board
                .post(post_req(&format!("task-{i}")))
                .await
                .expect("board post should succeed");
            board
                .add_stamp(stamp_params(
                    "rig-a", item.id, "Quality", 1.0, "Root", "human",
                ))
                .await
                .expect("add_stamp should succeed");
        }

        let level = board
            .trust_level(&RigId::new("rig-a"))
            .await
            .expect("trust_level should succeed");
        // 3 * Root(4.0) * 1.0 = ~12.0 => L2
        assert_eq!(level, "L2");
    }

    // ── stamps_with_scores ───────────────────────────────────

    #[tokio::test]
    async fn stamps_with_scores_empty_for_unknown_rig() {
        let board = new_board().await;
        let (stamps, dims, total) = board
            .stamps_with_scores(&RigId::new("nobody"))
            .await
            .expect("stamps_with_scores should succeed");
        assert!(stamps.is_empty());
        assert_eq!(total, 0.0);
        assert_eq!(dims.quality, 0.0);
        assert_eq!(dims.reliability, 0.0);
        assert_eq!(dims.helpfulness, 0.0);
        assert_eq!(dims.other, 0.0);
    }

    #[tokio::test]
    async fn stamps_with_scores_accumulates_dimensions() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.5, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Reliability",
                0.8,
                "Leaf",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Helpfulness",
                0.3,
                "Leaf",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Creativity",
                0.6,
                "Leaf",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let (stamps, dims, total) = board
            .stamps_with_scores(&RigId::new("rig-a"))
            .await
            .expect("stamps_with_scores should succeed");

        assert_eq!(stamps.len(), 4);
        assert!(dims.quality > 0.0);
        assert!(dims.reliability > 0.0);
        assert!(dims.helpfulness > 0.0);
        assert!(dims.other > 0.0, "custom dimension should go to other");
        assert!(total > 0.0);
        // total should approximately equal sum of dimensions
        let dim_sum = dims.quality + dims.reliability + dims.helpfulness + dims.other;
        assert!((total - dim_sum).abs() < 0.001);
    }

    // ── batch_rig_scores ─────────────────────────────────────

    #[tokio::test]
    async fn batch_rig_scores_empty_board() {
        let board = new_board().await;
        let scores = board
            .batch_rig_scores()
            .await
            .expect("batch_rig_scores should succeed");
        assert!(scores.is_empty());
    }

    #[tokio::test]
    async fn batch_rig_scores_returns_all_rigs() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.5, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-b", item.id, "Quality", 0.8, "Root", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Reliability",
                0.3,
                "Leaf",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let scores = board
            .batch_rig_scores()
            .await
            .expect("batch_rig_scores should succeed");

        assert_eq!(scores.len(), 2);
        assert!(scores.contains_key("rig-a"));
        assert!(scores.contains_key("rig-b"));

        let (score_a, level_a) = scores["rig-a"];
        let (score_b, level_b) = scores["rig-b"];
        assert!(score_a > 0.0);
        assert!(score_b > 0.0);
        // rig-a: Leaf(1.0)*0.5 + Leaf(1.0)*0.3 = 0.8 => L1
        assert_eq!(level_a, "L1");
        // rig-b: Root(4.0)*0.8 = 3.2 => L1.5
        assert_eq!(level_b, "L1.5");
        // rig-b has Root(4.0)*0.8=3.2, so score_b > score_a
        assert!(score_b > score_a);
    }

    // ── unprocessed_low_stamps ────────────────────────────────

    #[tokio::test]
    async fn unprocessed_low_stamps_returns_only_unevolved() {
        let board = Board::in_memory()
            .await
            .expect("in-memory board should initialize");
        let item = board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");

        let id1 = board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.2, "Leaf", "human",
            ))
            .await
            .expect("board operation should succeed");
        let _id2 = board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Reliability",
                0.8,
                "Leaf",
                "human",
            ))
            .await
            .expect("board operation should succeed");

        let low = board
            .unprocessed_low_stamps(0.3)
            .await
            .expect("async operation should succeed");
        assert_eq!(low.len(), 1);
        assert_eq!(low[0].id, id1);

        board
            .mark_stamp_evolved(id1)
            .await
            .expect("async operation should succeed");
        let low = board
            .unprocessed_low_stamps(0.3)
            .await
            .expect("async operation should succeed");
        assert!(low.is_empty());
    }

    #[tokio::test]
    async fn unprocessed_low_stamps_empty_board() {
        let board = new_board().await;
        let low = board
            .unprocessed_low_stamps(0.5)
            .await
            .expect("should succeed on empty board");
        assert!(low.is_empty());
    }

    // ── recent_low_stamps ────────────────────────────────────

    #[tokio::test]
    async fn recent_low_stamps_returns_recent_entries() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.1, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Reliability",
                0.9,
                "Leaf",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let low = board
            .recent_low_stamps(0.5, 7)
            .await
            .expect("recent_low_stamps should succeed");
        assert_eq!(low.len(), 1);
        assert!(low[0].score < 0.5);
    }

    #[tokio::test]
    async fn recent_low_stamps_empty_when_no_matches() {
        let board = new_board().await;
        let low = board
            .recent_low_stamps(0.5, 7)
            .await
            .expect("recent_low_stamps should succeed on empty board");
        assert!(low.is_empty());
    }

    #[tokio::test]
    async fn recent_low_stamps_respects_threshold() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.4, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        // threshold below the stamp score => no results
        let low = board
            .recent_low_stamps(0.3, 7)
            .await
            .expect("recent_low_stamps should succeed");
        assert!(low.is_empty());

        // threshold above the stamp score => one result
        let low = board
            .recent_low_stamps(0.5, 7)
            .await
            .expect("recent_low_stamps should succeed");
        assert_eq!(low.len(), 1);
    }

    // ── mark_stamp_evolved ───────────────────────────────────

    #[tokio::test]
    async fn mark_stamp_evolved_returns_false_for_nonexistent() {
        let board = new_board().await;
        let result = board
            .mark_stamp_evolved(9999)
            .await
            .expect("mark_stamp_evolved should succeed");
        assert!(!result);
    }

    #[tokio::test]
    async fn mark_stamp_evolved_idempotent() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");
        let stamp_id = board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.2, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let first = board
            .mark_stamp_evolved(stamp_id)
            .await
            .expect("mark_stamp_evolved should succeed");
        assert!(first, "first evolution should return true");

        let second = board
            .mark_stamp_evolved(stamp_id)
            .await
            .expect("mark_stamp_evolved should succeed");
        assert!(
            !second,
            "second evolution should return false (already evolved)"
        );
    }

    // ── Edge cases: weighted_score with different severities ──

    #[tokio::test]
    async fn weighted_score_severity_weights_applied_correctly() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        // Single Leaf stamp: weight 1.0, score 1.0 => ~1.0
        board
            .add_stamp(stamp_params(
                "rig-leaf", item.id, "Quality", 1.0, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        let leaf_score = board
            .weighted_score(&RigId::new("rig-leaf"))
            .await
            .expect("weighted_score should succeed");

        // Single Branch stamp: weight 2.0, score 1.0 => ~2.0
        board
            .add_stamp(stamp_params(
                "rig-branch",
                item.id,
                "Quality",
                1.0,
                "Branch",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");
        let branch_score = board
            .weighted_score(&RigId::new("rig-branch"))
            .await
            .expect("weighted_score should succeed");

        // Single Root stamp: weight 4.0, score 1.0 => ~4.0
        board
            .add_stamp(stamp_params(
                "rig-root", item.id, "Quality", 1.0, "Root", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        let root_score = board
            .weighted_score(&RigId::new("rig-root"))
            .await
            .expect("weighted_score should succeed");

        // Ratios should reflect severity weights (near-zero decay for fresh stamps)
        assert!(
            (branch_score / leaf_score - 2.0).abs() < 0.01,
            "Branch/Leaf ratio should be ~2.0, got {}",
            branch_score / leaf_score
        );
        assert!(
            (root_score / leaf_score - 4.0).abs() < 0.01,
            "Root/Leaf ratio should be ~4.0, got {}",
            root_score / leaf_score
        );
    }

    #[tokio::test]
    async fn weighted_score_positive_and_negative_cancel_out() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        // +0.5 Leaf and -0.5 Leaf should approximately cancel
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.5, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", -0.5, "Leaf", "rig-b",
            ))
            .await
            .expect("add_stamp should succeed");

        let score = board
            .weighted_score(&RigId::new("rig-a"))
            .await
            .expect("weighted_score should succeed");
        assert!(
            score.abs() < 0.01,
            "opposing stamps should cancel, got {score}"
        );
    }

    #[tokio::test]
    async fn weighted_score_zero_score_contributes_nothing() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.0, "Root", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let score = board
            .weighted_score(&RigId::new("rig-a"))
            .await
            .expect("weighted_score should succeed");
        assert_eq!(score, 0.0, "zero score stamp should contribute nothing");
    }

    // ── Edge cases: trust_level boundary transitions ─────────

    #[tokio::test]
    async fn trust_level_all_boundaries() {
        let board = new_board().await;

        // Helper: add N Root stamps (each ~4.0 weighted) to reach target score
        // L1: score < 3.0 (0 stamps)
        let level = board
            .trust_level(&RigId::new("rig-fresh"))
            .await
            .expect("trust_level should succeed");
        assert_eq!(level, "L1", "no stamps => L1");

        // L1.5: score >= 3.0 (1 Root stamp = ~4.0)
        let item1 = board
            .post(post_req("t1"))
            .await
            .expect("board post should succeed");
        board
            .add_stamp(stamp_params(
                "rig-l15", item1.id, "Quality", 1.0, "Root", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        let level = board
            .trust_level(&RigId::new("rig-l15"))
            .await
            .expect("trust_level should succeed");
        assert_eq!(level, "L1.5", "score ~4.0 => L1.5");

        // L2: score >= 10.0 (3 Root stamps = ~12.0)
        for i in 0..3 {
            let item = board
                .post(post_req(&format!("l2-{i}")))
                .await
                .expect("board post should succeed");
            board
                .add_stamp(stamp_params(
                    "rig-l2", item.id, "Quality", 1.0, "Root", "human",
                ))
                .await
                .expect("add_stamp should succeed");
        }
        let level = board
            .trust_level(&RigId::new("rig-l2"))
            .await
            .expect("trust_level should succeed");
        assert_eq!(level, "L2", "score ~12.0 => L2");

        // L2.5: score >= 25.0 (7 Root stamps = ~28.0)
        for i in 0..7 {
            let item = board
                .post(post_req(&format!("l25-{i}")))
                .await
                .expect("board post should succeed");
            board
                .add_stamp(stamp_params(
                    "rig-l25", item.id, "Quality", 1.0, "Root", "human",
                ))
                .await
                .expect("add_stamp should succeed");
        }
        let level = board
            .trust_level(&RigId::new("rig-l25"))
            .await
            .expect("trust_level should succeed");
        assert_eq!(level, "L2.5", "score ~28.0 => L2.5");

        // L3: score >= 50.0 (13 Root stamps = ~52.0)
        for i in 0..13 {
            let item = board
                .post(post_req(&format!("l3-{i}")))
                .await
                .expect("board post should succeed");
            board
                .add_stamp(stamp_params(
                    "rig-l3", item.id, "Quality", 1.0, "Root", "human",
                ))
                .await
                .expect("add_stamp should succeed");
        }
        let level = board
            .trust_level(&RigId::new("rig-l3"))
            .await
            .expect("trust_level should succeed");
        assert_eq!(level, "L3", "score ~52.0 => L3");
    }

    #[tokio::test]
    async fn trust_level_negative_stamps_keep_l1() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-bad", item.id, "Quality", -1.0, "Root", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let level = board
            .trust_level(&RigId::new("rig-bad"))
            .await
            .expect("trust_level should succeed");
        assert_eq!(level, "L1", "negative score stays at L1");
    }

    // ── Edge cases: multiple stamps on same item ─────────────

    #[tokio::test]
    async fn multiple_stamps_same_item_different_dimensions() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        for dim in ["Quality", "Reliability", "Helpfulness"] {
            board
                .add_stamp(stamp_params("rig-a", item.id, dim, 0.8, "Leaf", "human"))
                .await
                .expect("add_stamp should succeed");
        }

        let stamps = board
            .stamps_for_item(item.id)
            .await
            .expect("stamps_for_item should succeed");
        assert_eq!(stamps.len(), 3, "3 stamps on one item");

        let (_, dims, total) = board
            .stamps_with_scores(&RigId::new("rig-a"))
            .await
            .expect("stamps_with_scores should succeed");
        assert!(dims.quality > 0.0);
        assert!(dims.reliability > 0.0);
        assert!(dims.helpfulness > 0.0);
        assert_eq!(dims.other, 0.0, "no custom dimensions used");
        let dim_sum = dims.quality + dims.reliability + dims.helpfulness + dims.other;
        assert!(
            (total - dim_sum).abs() < 0.001,
            "total should equal dimension sum"
        );
    }

    #[tokio::test]
    async fn multiple_stamps_same_item_same_dimension_accumulate() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        // Two Quality stamps on same item
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.5, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.3, "Leaf", "rig-b",
            ))
            .await
            .expect("add_stamp should succeed");

        let stamps = board
            .stamps_for_item(item.id)
            .await
            .expect("stamps_for_item should succeed");
        assert_eq!(stamps.len(), 2, "both stamps recorded");

        let (_, dims, _) = board
            .stamps_with_scores(&RigId::new("rig-a"))
            .await
            .expect("stamps_with_scores should succeed");
        // quality should reflect both stamps: ~0.5 + ~0.3 = ~0.8
        assert!(
            dims.quality > 0.7,
            "quality should accumulate, got {}",
            dims.quality
        );
    }

    // ── Edge cases: stamps_with_scores dimension routing ─────

    #[tokio::test]
    async fn stamps_with_scores_mixed_severities() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 1.0, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 1.0, "Root", "rig-b",
            ))
            .await
            .expect("add_stamp should succeed");

        let (stamps, dims, total) = board
            .stamps_with_scores(&RigId::new("rig-a"))
            .await
            .expect("stamps_with_scores should succeed");
        assert_eq!(stamps.len(), 2);
        // Leaf(1.0)*1.0 + Root(4.0)*1.0 = ~5.0
        assert!(
            dims.quality > 4.5,
            "quality should reflect severity weights, got {}",
            dims.quality
        );
        assert!(
            (total - dims.quality).abs() < 0.001,
            "all stamps in Quality => total == quality"
        );
    }

    #[tokio::test]
    async fn stamps_with_scores_negative_dims() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", -0.8, "Branch", "human",
            ))
            .await
            .expect("add_stamp should succeed");
        board
            .add_stamp(stamp_params(
                "rig-a",
                item.id,
                "Reliability",
                0.5,
                "Leaf",
                "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let (_, dims, total) = board
            .stamps_with_scores(&RigId::new("rig-a"))
            .await
            .expect("stamps_with_scores should succeed");
        // Quality: Branch(2.0) * -0.8 = -1.6
        assert!(
            dims.quality < 0.0,
            "negative quality expected, got {}",
            dims.quality
        );
        // Reliability: Leaf(1.0) * 0.5 = 0.5
        assert!(dims.reliability > 0.0);
        // total = -1.6 + 0.5 = -1.1
        assert!(total < 0.0, "net negative total expected, got {total}");
        let dim_sum = dims.quality + dims.reliability + dims.helpfulness + dims.other;
        assert!(
            (total - dim_sum).abs() < 0.001,
            "total should equal dimension sum"
        );
    }

    // ── Edge cases: batch_rig_scores ─────────────────────────

    #[tokio::test]
    async fn batch_rig_scores_negative_score_stays_l1() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-neg", item.id, "Quality", -1.0, "Root", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let scores = board
            .batch_rig_scores()
            .await
            .expect("batch_rig_scores should succeed");
        let (score, level) = scores["rig-neg"];
        assert!(score < 0.0, "score should be negative, got {score}");
        assert_eq!(level, "L1", "negative score => L1");
    }

    #[tokio::test]
    async fn batch_rig_scores_multiple_stamps_per_rig() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        // rig-a gets 3 Leaf stamps of 0.5 => ~1.5 total
        for stamper in ["h1", "h2", "h3"] {
            board
                .add_stamp(stamp_params(
                    "rig-a", item.id, "Quality", 0.5, "Leaf", stamper,
                ))
                .await
                .expect("add_stamp should succeed");
        }

        let scores = board
            .batch_rig_scores()
            .await
            .expect("batch_rig_scores should succeed");
        let (score, level) = scores["rig-a"];
        assert!(
            score > 1.4 && score <= 1.5,
            "3 * Leaf * 0.5 should be ~1.5, got {score}"
        );
        assert_eq!(level, "L1", "score ~1.5 => L1");
    }

    // ── Edge cases: empty stamp lists ────────────────────────

    #[tokio::test]
    async fn stamps_for_rig_empty_string_rig_id() {
        let board = new_board().await;
        let stamps = board
            .stamps_for_rig(&RigId::new(""))
            .await
            .expect("stamps_for_rig should succeed for empty rig id");
        assert!(stamps.is_empty());
    }

    #[tokio::test]
    async fn weighted_score_empty_string_rig() {
        let board = new_board().await;
        let score = board
            .weighted_score(&RigId::new(""))
            .await
            .expect("weighted_score should succeed for empty rig id");
        assert_eq!(score, 0.0);
    }

    #[tokio::test]
    async fn stamps_with_scores_empty_string_rig() {
        let board = new_board().await;
        let (stamps, dims, total) = board
            .stamps_with_scores(&RigId::new(""))
            .await
            .expect("stamps_with_scores should succeed for empty rig id");
        assert!(stamps.is_empty());
        assert_eq!(total, 0.0);
        assert_eq!(dims.quality, 0.0);
    }

    // ── Edge cases: unprocessed_low_stamps threshold ─────────

    #[tokio::test]
    async fn unprocessed_low_stamps_threshold_boundary() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        // Add stamp with score exactly at threshold value
        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", 0.5, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        // Threshold equal to score: lt means score < threshold, so 0.5 < 0.5 is false
        let low = board
            .unprocessed_low_stamps(0.5)
            .await
            .expect("unprocessed_low_stamps should succeed");
        assert!(
            low.is_empty(),
            "score == threshold should not match (lt, not lte)"
        );

        // Threshold just above score
        let low = board
            .unprocessed_low_stamps(0.51)
            .await
            .expect("unprocessed_low_stamps should succeed");
        assert_eq!(low.len(), 1, "score < threshold should match");
    }

    #[tokio::test]
    async fn unprocessed_low_stamps_negative_scores_found() {
        let board = new_board().await;
        let item = board
            .post(post_req("task"))
            .await
            .expect("board post should succeed");

        board
            .add_stamp(stamp_params(
                "rig-a", item.id, "Quality", -0.5, "Leaf", "human",
            ))
            .await
            .expect("add_stamp should succeed");

        let low = board
            .unprocessed_low_stamps(0.0)
            .await
            .expect("unprocessed_low_stamps should succeed");
        assert_eq!(
            low.len(),
            1,
            "negative score should be found below threshold 0.0"
        );
    }
}
