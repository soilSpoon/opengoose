// Stamp operations for Board.

use crate::board::{AddStampParams, Board, stamp_weighted_value};
use crate::entity;
use crate::stamps::{Severity, TrustLevel};
use crate::work_item::BoardError;
use chrono::Utc;
use sea_orm::*;

fn db_err(e: DbErr) -> BoardError {
    BoardError::DbError(e.to_string())
}

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
        rig_id: &str,
    ) -> Result<Vec<entity::stamp::Model>, BoardError> {
        entity::stamp::Entity::find()
            .filter(entity::stamp::Column::TargetRig.eq(rig_id))
            .all(&self.db)
            .await
            .map_err(db_err)
    }

    /// 가중 점수 (시간 감쇠 적용). 30일 반감기.
    pub async fn weighted_score(&self, rig_id: &str) -> Result<f32, BoardError> {
        let stamps = entity::stamp::Entity::find()
            .filter(entity::stamp::Column::TargetRig.eq(rig_id))
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let now = Utc::now();
        Ok(stamps.iter().map(|s| stamp_weighted_value(s, now)).sum())
    }

    /// 신뢰 수준. stamps.rs의 TrustLevel::from_score() 재사용.
    pub async fn trust_level(&self, rig_id: &str) -> Result<&'static str, BoardError> {
        let score = self.weighted_score(rig_id).await?;
        Ok(TrustLevel::from_score(score).as_str())
    }

    /// 특정 rig의 stamps + 차원별/전체 가중 점수를 한 번에 조회.
    /// 반환: (stamps, [quality, reliability, helpfulness], total_score)
    pub async fn stamps_with_scores(
        &self,
        rig_id: &str,
    ) -> Result<(Vec<entity::stamp::Model>, [f32; 3], f32), BoardError> {
        let stamps = entity::stamp::Entity::find()
            .filter(entity::stamp::Column::TargetRig.eq(rig_id))
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let now = Utc::now();
        let mut dim_scores = [0.0_f32; 3]; // [quality, reliability, helpfulness]
        let mut total = 0.0_f32;

        for s in &stamps {
            let weighted = stamp_weighted_value(s, now);
            total += weighted;
            match s.dimension.as_str() {
                "Quality" => dim_scores[0] += weighted,
                "Reliability" => dim_scores[1] += weighted,
                "Helpfulness" => dim_scores[2] += weighted,
                _ => {}
            }
        }

        Ok((stamps, dim_scores, total))
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
        let mut scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

        for s in &stamps {
            *scores.entry(s.target_rig.clone()).or_default() += stamp_weighted_value(s, now);
        }

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
    use crate::work_item::{PostWorkItem, Priority, RigId};

    async fn new_board() -> Board {
        Board::in_memory().await.unwrap()
    }

    fn post_req(title: &str) -> PostWorkItem {
        PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
        }
    }

    fn stamp_params<'a>(
        target_rig: &'a str,
        work_item_id: i64,
        dimension: &'a str,
        score: f32,
        severity: &'a str,
        stamped_by: &'a str,
    ) -> AddStampParams<'a> {
        AddStampParams {
            target_rig,
            work_item_id,
            dimension,
            score,
            severity,
            stamped_by,
            comment: None,
            active_skill_versions: None,
        }
    }

    #[tokio::test]
    async fn stamp_yearbook_rule_enforced_db() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        let result = board
            .add_stamp(stamp_params("rig-a", item.id, "Quality", 0.5, "Leaf", "rig-a"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stamp_invalid_score_rejected_db() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        let result = board
            .add_stamp(stamp_params("rig-a", item.id, "Quality", 1.5, "Leaf", "rig-b"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stamp_invalid_severity_rejected_db() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        let result = board
            .add_stamp(stamp_params("rig-a", item.id, "Quality", 0.5, "Invalid", "rig-b"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stamp_custom_dimension_accepted() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        let result = board
            .add_stamp(stamp_params("rig-a", item.id, "Creativity", 0.5, "Leaf", "rig-b"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn unprocessed_low_stamps_returns_only_unevolved() {
        let board = Board::in_memory().await.unwrap();
        let item = board.post(post_req("test")).await.unwrap();

        let id1 = board
            .add_stamp(stamp_params("rig-a", item.id, "Quality", 0.2, "Leaf", "human"))
            .await
            .unwrap();
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
            .unwrap();

        let low = board.unprocessed_low_stamps(0.3).await.unwrap();
        assert_eq!(low.len(), 1);
        assert_eq!(low[0].id, id1);

        board.mark_stamp_evolved(id1).await.unwrap();
        let low = board.unprocessed_low_stamps(0.3).await.unwrap();
        assert!(low.is_empty());
    }

    #[tokio::test]
    async fn stamps_for_rig_returns_all_for_rig() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();

        board
            .add_stamp(stamp_params("rig-x", item.id, "Quality", 0.5, "Leaf", "human"))
            .await
            .unwrap();
        board
            .add_stamp(stamp_params("rig-x", item.id, "Reliability", 0.8, "Leaf", "human"))
            .await
            .unwrap();
        board
            .add_stamp(stamp_params("rig-y", item.id, "Quality", 0.3, "Leaf", "human"))
            .await
            .unwrap();

        let stamps = board.stamps_for_rig("rig-x").await.unwrap();
        assert_eq!(stamps.len(), 2);

        let stamps_y = board.stamps_for_rig("rig-y").await.unwrap();
        assert_eq!(stamps_y.len(), 1);

        let empty = board.stamps_for_rig("nobody").await.unwrap();
        assert!(empty.is_empty());
    }
}
