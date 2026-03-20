// Wanted Board — SQLite 기반 pull 작업 분배
//
// SeaORM + SQLite. 모든 메서드가 async.
// 상태 변경 메서드는 트랜잭션으로 원자성 보장.

use crate::entity;
use crate::stamps::{Severity, TrustLevel};
use crate::work_item::{BoardError, PostWorkItem, RigId, Status, WorkItem};
use chrono::{DateTime, Utc};
use sea_orm::*;
use std::sync::Arc;
use tokio::sync::Notify;

/// Parameters for adding a stamp.
pub struct AddStampParams<'a> {
    pub target_rig: &'a str,
    pub work_item_id: i64,
    pub dimension: &'a str,
    pub score: f32,
    pub severity: &'a str,
    pub stamped_by: &'a str,
    pub comment: Option<&'a str>,
    pub active_skill_versions: Option<&'a str>,
}

/// stamp의 가중 점수 (시간 감쇠). 30일 반감기.
fn stamp_weighted_value(stamp: &entity::stamp::Model, now: DateTime<Utc>) -> f32 {
    let days = (now - stamp.timestamp).num_seconds() as f32 / 86400.0;
    let decay = 0.5_f32.powf(days / 30.0);
    let weight = Severity::parse(&stamp.severity)
        .unwrap_or(Severity::Leaf)
        .weight();
    weight * stamp.score * decay
}

pub struct Board {
    db: DatabaseConnection,
    notify: Arc<Notify>,
    stamp_notify: Arc<Notify>,
}

impl Board {
    pub async fn connect(db_url: &str) -> Result<Self, BoardError> {
        let db = Database::connect(db_url).await.map_err(db_err)?;
        Self::create_tables(&db).await?;
        Self::ensure_columns(&db).await?;
        Self::ensure_system_rigs(&db).await?;
        Ok(Self {
            db,
            notify: Arc::new(Notify::new()),
            stamp_notify: Arc::new(Notify::new()),
        })
    }

    pub async fn in_memory() -> Result<Self, BoardError> {
        Self::connect("sqlite::memory:").await
    }

    async fn create_tables(db: &DatabaseConnection) -> Result<(), BoardError> {
        let backend = db.get_database_backend();
        let schema = Schema::new(backend);

        for mut stmt in [
            schema.create_table_from_entity(entity::work_item::Entity),
            schema.create_table_from_entity(entity::relation::Entity),
            schema.create_table_from_entity(entity::stamp::Entity),
            schema.create_table_from_entity(entity::rig::Entity),
        ] {
            let sql = backend.build(&stmt.if_not_exists().to_owned());
            db.execute_raw(sql).await.map_err(db_err)?;
        }
        Ok(())
    }

    async fn ensure_columns(db: &DatabaseConnection) -> Result<(), BoardError> {
        // Idempotent: ignore "duplicate column" errors for existing databases
        let stmts = ["ALTER TABLE stamps ADD COLUMN active_skill_versions TEXT"];
        for sql in stmts {
            let _ = db.execute_unprepared(sql).await;
        }
        Ok(())
    }

    // ── 기본 API ─────────────────────────────────────────────

    pub async fn post(&self, req: PostWorkItem) -> Result<WorkItem, BoardError> {
        let now = Utc::now();
        let tags_json = if req.tags.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&req.tags).map_err(|e| BoardError::DbError(e.to_string()))?)
        };
        let model = entity::work_item::ActiveModel {
            id: NotSet,
            title: Set(req.title),
            description: Set(req.description),
            status: Set(Status::Open),
            priority: Set(req.priority),
            tags: Set(tags_json),
            created_by: Set(req.created_by.0),
            claimed_by: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };

        let result = entity::work_item::Entity::insert(model)
            .exec(&self.db)
            .await
            .map_err(db_err)?;

        self.notify.notify_waiters();
        self.get_or_err(result.last_insert_id).await
    }

    pub async fn claim(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let txn = self.db.begin().await.map_err(db_err)?;

        let model = Self::find_model(&txn, item_id).await?;
        let item = WorkItem::from(model.clone());

        if item.status == Status::Claimed {
            return Err(BoardError::AlreadyClaimed {
                id: item_id,
                claimed_by: item.claimed_by.unwrap_or_else(|| RigId::new("unknown")),
            });
        }
        item.status.validate_transition(Status::Claimed)?;

        let mut active: entity::work_item::ActiveModel = model.into();
        active.status = Set(Status::Claimed);
        active.claimed_by = Set(Some(rig_id.0.clone()));
        active.updated_at = Set(Utc::now());
        let updated = active.update(&txn).await.map_err(db_err)?;

        txn.commit().await.map_err(db_err)?;
        Ok(WorkItem::from(updated))
    }

    pub async fn submit(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let result = self
            .transition(
                item_id,
                Status::Done,
                |item| item.verify_claimed_by(rig_id),
                |_| {},
            )
            .await?;
        self.notify.notify_waiters();
        Ok(result)
    }

    pub async fn unclaim(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let rig_id = rig_id.clone();
        let result = self
            .transition(
                item_id,
                Status::Open,
                |item| item.verify_claimed_by(&rig_id),
                |active| {
                    active.claimed_by = Set(None);
                },
            )
            .await?;
        self.notify.notify_waiters();
        Ok(result)
    }

    pub async fn mark_stuck(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let rig_id = rig_id.clone();
        self.transition(
            item_id,
            Status::Stuck,
            |item| {
                if let Some(ref claimed) = item.claimed_by
                    && claimed != &rig_id
                {
                    return Err(BoardError::NotClaimedBy {
                        id: item.id,
                        claimed_by: claimed.clone(),
                        attempted_by: rig_id.clone(),
                    });
                }
                Ok(())
            },
            |_| {},
        )
        .await
    }

    pub async fn retry(&self, item_id: i64) -> Result<WorkItem, BoardError> {
        let result = self
            .transition(
                item_id,
                Status::Open,
                |_| Ok(()),
                |active| {
                    active.claimed_by = Set(None);
                },
            )
            .await?;
        self.notify.notify_waiters();
        Ok(result)
    }

    pub async fn abandon(&self, item_id: i64) -> Result<WorkItem, BoardError> {
        let result = self
            .transition(item_id, Status::Abandoned, |_| Ok(()), |_| {})
            .await?;
        self.notify.notify_waiters();
        Ok(result)
    }

    // ── 조회 ─────────────────────────────────────────────────

    pub async fn get(&self, item_id: i64) -> Result<Option<WorkItem>, BoardError> {
        entity::work_item::Entity::find_by_id(item_id)
            .one(&self.db)
            .await
            .map(|opt| opt.map(WorkItem::from))
            .map_err(db_err)
    }

    pub async fn list(&self) -> Result<Vec<WorkItem>, BoardError> {
        entity::work_item::Entity::find()
            .all(&self.db)
            .await
            .map(|models| models.into_iter().map(WorkItem::from).collect())
            .map_err(db_err)
    }

    pub async fn ready(&self) -> Result<Vec<WorkItem>, BoardError> {
        let blocked_ids = self.blocked_item_ids().await?;

        let mut items: Vec<WorkItem> = entity::work_item::Entity::find()
            .filter(entity::work_item::Column::Status.eq(Status::Open.to_value()))
            .all(&self.db)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(WorkItem::from)
            .filter(|item| !blocked_ids.contains(&item.id))
            .collect();

        items.sort_by(|a, b| b.priority.urgency().cmp(&a.priority.urgency()));
        Ok(items)
    }

    /// 특정 rig이 claim한 아이템 조회. priority 내림차순.
    pub async fn claimed_by(&self, rig_id: &RigId) -> Result<Vec<WorkItem>, BoardError> {
        let mut items: Vec<WorkItem> = entity::work_item::Entity::find()
            .filter(entity::work_item::Column::Status.eq(Status::Claimed.to_value()))
            .filter(entity::work_item::Column::ClaimedBy.eq(&rig_id.0))
            .all(&self.db)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(WorkItem::from)
            .collect();

        items.sort_by(|a, b| b.priority.urgency().cmp(&a.priority.urgency()));
        Ok(items)
    }

    /// 특정 rig이 완료한 작업 항목 조회 (SQL 필터).
    pub async fn completed_by_rig(&self, rig_id: &str) -> Result<Vec<WorkItem>, BoardError> {
        entity::work_item::Entity::find()
            .filter(entity::work_item::Column::Status.eq(Status::Done.to_value()))
            .filter(entity::work_item::Column::ClaimedBy.eq(rig_id))
            .all(&self.db)
            .await
            .map(|models| models.into_iter().map(WorkItem::from).collect())
            .map_err(db_err)
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

    // ── Rigs ──────────────────────────────────────────────────

    pub async fn register_rig(
        &self,
        id: &str,
        rig_type: &str,
        recipe: Option<&str>,
        tags: Option<&[String]>,
    ) -> Result<(), BoardError> {
        let tags_json = tags
            .map(|t| serde_json::to_string(t).map_err(|e| BoardError::DbError(e.to_string())))
            .transpose()?;

        let model = entity::rig::ActiveModel {
            id: Set(id.to_string()),
            rig_type: Set(rig_type.to_string()),
            recipe: Set(recipe.map(|s| s.to_string())),
            tags: Set(tags_json),
            created_at: Set(chrono::Utc::now()),
        };

        // upsert: 이미 있으면 무시 (멱등)
        if self.get_rig(id).await?.is_some() {
            return Ok(());
        }
        entity::rig::Entity::insert(model)
            .exec(&self.db)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    pub async fn list_rigs(&self) -> Result<Vec<entity::rig::Model>, BoardError> {
        entity::rig::Entity::find()
            .all(&self.db)
            .await
            .map_err(db_err)
    }

    pub async fn get_rig(&self, id: &str) -> Result<Option<entity::rig::Model>, BoardError> {
        entity::rig::Entity::find_by_id(id.to_string())
            .one(&self.db)
            .await
            .map_err(db_err)
    }

    pub async fn remove_rig(&self, id: &str) -> Result<(), BoardError> {
        // system rig 삭제 방지
        if let Some(rig) = self.get_rig(id).await?
            && rig.rig_type == "system"
        {
            return Err(BoardError::SystemRigProtected(id.to_string()));
        }
        entity::rig::Entity::delete_by_id(id.to_string())
            .exec(&self.db)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    // ── Relations ────────────────────────────────────────────

    pub async fn add_dependency(&self, blocker: i64, blocked: i64) -> Result<(), BoardError> {
        if blocker == blocked {
            return Err(BoardError::CyclicDependency(vec![blocker, blocked]));
        }
        if self.would_create_cycle(blocker, blocked).await? {
            return Err(BoardError::CyclicDependency(vec![blocker, blocked]));
        }

        entity::relation::Entity::insert(entity::relation::ActiveModel {
            id: NotSet,
            from_id: Set(blocker),
            to_id: Set(blocked),
            relation_type: Set("Blocks".to_string()),
        })
        .exec(&self.db)
        .await
        .map_err(db_err)?;

        Ok(())
    }

    // ── Stamps ────────────────────────────────────────────────

    pub async fn add_stamp(&self, p: AddStampParams<'_>) -> Result<i64, BoardError> {
        // 졸업앨범 규칙
        if p.stamped_by == p.target_rig {
            return Err(BoardError::YearbookViolation {
                stamper: RigId::new(p.stamped_by),
                target: RigId::new(p.target_rig),
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

    // ── 알림 ─────────────────────────────────────────────────

    pub async fn wait_for_claimable(&self) {
        self.notify.notified().await;
    }

    pub fn notify_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub fn stamp_notify_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.stamp_notify)
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

    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    // ── 내부 헬퍼 ────────────────────────────────────────────

    /// 공통 상태 전이 패턴. txn begin → find → validate → apply → commit.
    async fn transition(
        &self,
        item_id: i64,
        target: Status,
        validate: impl FnOnce(&WorkItem) -> Result<(), BoardError>,
        apply: impl FnOnce(&mut entity::work_item::ActiveModel),
    ) -> Result<WorkItem, BoardError> {
        let txn = self.db.begin().await.map_err(db_err)?;
        let model = Self::find_model(&txn, item_id).await?;
        let item = WorkItem::from(model.clone());
        validate(&item)?;
        item.status.validate_transition(target)?;
        let mut active: entity::work_item::ActiveModel = model.into();
        active.status = Set(target);
        apply(&mut active);
        active.updated_at = Set(Utc::now());
        let updated = active.update(&txn).await.map_err(db_err)?;
        txn.commit().await.map_err(db_err)?;
        Ok(WorkItem::from(updated))
    }

    async fn get_or_err(&self, item_id: i64) -> Result<WorkItem, BoardError> {
        self.get(item_id)
            .await?
            .ok_or(BoardError::NotFound(item_id))
    }

    /// 트랜잭션 내부용: Model을 직접 반환 (ActiveModel 변환에 사용).
    async fn find_model<C: ConnectionTrait>(
        conn: &C,
        item_id: i64,
    ) -> Result<entity::work_item::Model, BoardError> {
        entity::work_item::Entity::find_by_id(item_id)
            .one(conn)
            .await
            .map_err(db_err)?
            .ok_or(BoardError::NotFound(item_id))
    }

    /// 블록된 아이템 ID 집합. 단일 배치 쿼리로 블로커 상태를 확인.
    async fn blocked_item_ids(&self) -> Result<std::collections::HashSet<i64>, BoardError> {
        let relations = entity::relation::Entity::find()
            .all(&self.db)
            .await
            .map_err(db_err)?;

        if relations.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        // 모든 블로커 ID를 수집하여 Done 상태인 것들을 배치 조회
        let blocker_ids: Vec<i64> = relations.iter().map(|r| r.from_id).collect();
        let done_blockers: std::collections::HashSet<i64> = entity::work_item::Entity::find()
            .filter(entity::work_item::Column::Id.is_in(blocker_ids))
            .filter(entity::work_item::Column::Status.eq(Status::Done.to_value()))
            .all(&self.db)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(|m| m.id)
            .collect();

        let mut blocked = std::collections::HashSet::new();
        for rel in &relations {
            if !done_blockers.contains(&rel.from_id) {
                blocked.insert(rel.to_id);
            }
        }
        Ok(blocked)
    }

    /// 순환 감지. 전체 relation 테이블을 한 번 로드하여 in-memory BFS.
    async fn would_create_cycle(&self, from: i64, to: i64) -> Result<bool, BoardError> {
        // 전체 relations를 한 번에 로드 (N+1 쿼리 방지)
        let all_relations = entity::relation::Entity::find()
            .all(&self.db)
            .await
            .map_err(db_err)?;

        // 역방향 인덱스: to_id → [from_id] (blockers_of 동일)
        let mut reverse: std::collections::HashMap<i64, Vec<i64>> =
            std::collections::HashMap::new();
        for rel in &all_relations {
            reverse.entry(rel.to_id).or_default().push(rel.from_id);
        }

        // from에서 시작, 역방향(blockers)을 따라 to에 도달하면 순환
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(from);

        while let Some(current) = queue.pop_front() {
            if current == to {
                return Ok(true);
            }
            if visited.insert(current)
                && let Some(blockers) = reverse.get(&current)
            {
                for &blocker_id in blockers {
                    if !visited.contains(&blocker_id) {
                        queue.push_back(blocker_id);
                    }
                }
            }
        }
        Ok(false)
    }
}

impl Board {
    async fn ensure_system_rigs(db: &DatabaseConnection) -> Result<(), BoardError> {
        for (id, rig_type) in [("human", "system"), ("evolver", "system")] {
            let existing = entity::rig::Entity::find_by_id(id.to_string())
                .one(db)
                .await
                .map_err(db_err)?;
            if existing.is_none() {
                entity::rig::Entity::insert(entity::rig::ActiveModel {
                    id: Set(id.to_string()),
                    rig_type: Set(rig_type.to_string()),
                    recipe: Set(None),
                    tags: Set(None),
                    created_at: Set(chrono::Utc::now()),
                })
                .exec(db)
                .await
                .map_err(db_err)?;
            }
        }
        Ok(())
    }
}

fn db_err(e: DbErr) -> BoardError {
    BoardError::DbError(e.to_string())
}

// ── 테스트 ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_item::Priority;

    async fn new_board() -> Board {
        Board::in_memory().await.unwrap()
    }

    fn stamp<'a>(
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

    fn post_req(title: &str) -> PostWorkItem {
        PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn post_creates_open_item() {
        let board = new_board().await;
        let item = board.post(post_req("test")).await.unwrap();
        assert_eq!(item.id, 1);
        assert_eq!(item.status, Status::Open);
        assert!(item.claimed_by.is_none());
    }

    #[tokio::test]
    async fn auto_increment_ids() {
        let board = new_board().await;
        let a = board.post(post_req("a")).await.unwrap();
        let b = board.post(post_req("b")).await.unwrap();
        assert_eq!(a.id, 1);
        assert_eq!(b.id, 2);
    }

    #[tokio::test]
    async fn claim_and_submit() {
        let board = new_board().await;
        board.post(post_req("test")).await.unwrap();

        let rig = RigId::new("dev");
        let claimed = board.claim(1, &rig).await.unwrap();
        assert_eq!(claimed.status, Status::Claimed);
        assert_eq!(claimed.claimed_by, Some(rig.clone()));

        let done = board.submit(1, &rig).await.unwrap();
        assert_eq!(done.status, Status::Done);
    }

    #[tokio::test]
    async fn claim_already_claimed_fails() {
        let board = new_board().await;
        board.post(post_req("test")).await.unwrap();
        board.claim(1, &RigId::new("dev")).await.unwrap();
        assert!(board.claim(1, &RigId::new("other")).await.is_err());
    }

    #[tokio::test]
    async fn submit_wrong_rig_fails() {
        let board = new_board().await;
        board.post(post_req("test")).await.unwrap();
        board.claim(1, &RigId::new("dev")).await.unwrap();
        assert!(board.submit(1, &RigId::new("other")).await.is_err());
    }

    #[tokio::test]
    async fn unclaim_returns_to_open() {
        let board = new_board().await;
        board.post(post_req("test")).await.unwrap();
        let rig = RigId::new("dev");
        board.claim(1, &rig).await.unwrap();

        let unclaimed = board.unclaim(1, &rig).await.unwrap();
        assert_eq!(unclaimed.status, Status::Open);
        assert!(unclaimed.claimed_by.is_none());
    }

    #[tokio::test]
    async fn stuck_and_retry() {
        let board = new_board().await;
        board.post(post_req("test")).await.unwrap();
        let rig = RigId::new("dev");
        board.claim(1, &rig).await.unwrap();

        board.mark_stuck(1, &rig).await.unwrap();
        assert_eq!(board.get(1).await.unwrap().unwrap().status, Status::Stuck);

        board.retry(1).await.unwrap();
        assert_eq!(board.get(1).await.unwrap().unwrap().status, Status::Open);
    }

    #[tokio::test]
    async fn abandon_from_open() {
        let board = new_board().await;
        board.post(post_req("test")).await.unwrap();
        board.abandon(1).await.unwrap();
        assert_eq!(
            board.get(1).await.unwrap().unwrap().status,
            Status::Abandoned
        );
    }

    #[tokio::test]
    async fn invalid_transition_fails() {
        let board = new_board().await;
        board.post(post_req("test")).await.unwrap();
        assert!(board.submit(1, &RigId::new("dev")).await.is_err());
    }

    #[tokio::test]
    async fn ready_excludes_blocked() {
        let board = new_board().await;
        let a = board.post(post_req("blocker")).await.unwrap();
        let b = board.post(post_req("blocked")).await.unwrap();
        board.add_dependency(a.id, b.id).await.unwrap();

        let ids: Vec<i64> = board.ready().await.unwrap().iter().map(|i| i.id).collect();
        assert!(ids.contains(&a.id));
        assert!(!ids.contains(&b.id));
    }

    #[tokio::test]
    async fn ready_unblocks_when_done() {
        let board = new_board().await;
        let a = board.post(post_req("blocker")).await.unwrap();
        let b = board.post(post_req("blocked")).await.unwrap();
        board.add_dependency(a.id, b.id).await.unwrap();

        board.claim(a.id, &RigId::new("dev")).await.unwrap();
        board.submit(a.id, &RigId::new("dev")).await.unwrap();

        let ids: Vec<i64> = board.ready().await.unwrap().iter().map(|i| i.id).collect();
        assert!(ids.contains(&b.id));
    }

    #[tokio::test]
    async fn ready_priority_sorted() {
        let board = new_board().await;
        board
            .post(PostWorkItem {
                title: "low".into(),
                description: String::new(),
                created_by: RigId::new("user"),
                priority: Priority::P2,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .post(PostWorkItem {
                title: "urgent".into(),
                description: String::new(),
                created_by: RigId::new("user"),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .unwrap();

        let ready = board.ready().await.unwrap();
        assert_eq!(ready[0].priority, Priority::P0);
        assert_eq!(ready[1].priority, Priority::P2);
    }

    #[tokio::test]
    async fn cycle_detection() {
        let board = new_board().await;
        board.post(post_req("a")).await.unwrap();
        board.post(post_req("b")).await.unwrap();
        board.post(post_req("c")).await.unwrap();

        board.add_dependency(1, 2).await.unwrap();
        board.add_dependency(2, 3).await.unwrap();
        assert!(board.add_dependency(3, 1).await.is_err());
    }

    #[tokio::test]
    async fn self_cycle_rejected() {
        let board = new_board().await;
        board.post(post_req("a")).await.unwrap();
        assert!(board.add_dependency(1, 1).await.is_err());
    }

    #[tokio::test]
    async fn wait_for_claimable_wakes() {
        let board = new_board().await;
        let notify = board.notify_handle();

        let handle = tokio::spawn(async move {
            notify.notified().await;
            true
        });

        tokio::task::yield_now().await;
        board.post(post_req("wake")).await.unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_returns_all() {
        let board = new_board().await;
        board.post(post_req("a")).await.unwrap();
        board.post(post_req("b")).await.unwrap();
        assert_eq!(board.list().await.unwrap().len(), 2);
    }

    // ── Task 6: Rig lifecycle tests ──────────────────────────────────

    #[tokio::test]
    async fn rig_lifecycle_register_stamp_trust() {
        let board = new_board().await;
        board
            .register_rig("ai-01", "ai", Some("developer"), Some(&["rust".into()]))
            .await
            .unwrap();
        let rig = board.get_rig("ai-01").await.unwrap().unwrap();
        assert_eq!(rig.rig_type, "ai");

        let level = board.trust_level("ai-01").await.unwrap();
        assert_eq!(level, "L1");

        let item = board.post(post_req("task 1")).await.unwrap();
        board.claim(item.id, &RigId::new("ai-01")).await.unwrap();
        board.submit(item.id, &RigId::new("ai-01")).await.unwrap();

        board
            .add_stamp(stamp("ai-01", item.id, "Quality", 1.0, "Root", "reviewer"))
            .await
            .unwrap();
        let level = board.trust_level("ai-01").await.unwrap();
        assert_eq!(level, "L1.5");

        let item2 = board.post(post_req("task 2")).await.unwrap();
        board.claim(item2.id, &RigId::new("ai-01")).await.unwrap();
        board.submit(item2.id, &RigId::new("ai-01")).await.unwrap();
        board
            .add_stamp(stamp(
                "ai-01",
                item2.id,
                "Reliability",
                1.0,
                "Root",
                "reviewer",
            ))
            .await
            .unwrap();
        board
            .add_stamp(stamp(
                "ai-01",
                item2.id,
                "Helpfulness",
                1.0,
                "Branch",
                "reviewer",
            ))
            .await
            .unwrap();
        let level = board.trust_level("ai-01").await.unwrap();
        assert_eq!(level, "L2");
    }

    #[tokio::test]
    async fn stamp_yearbook_rule_enforced_db() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        let result = board
            .add_stamp(stamp("rig-a", item.id, "Quality", 0.5, "Leaf", "rig-a"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stamp_invalid_score_rejected_db() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        let result = board
            .add_stamp(stamp("rig-a", item.id, "Quality", 1.5, "Leaf", "rig-b"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stamp_invalid_severity_rejected_db() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        let result = board
            .add_stamp(stamp("rig-a", item.id, "Quality", 0.5, "Invalid", "rig-b"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stamp_custom_dimension_accepted() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        let result = board
            .add_stamp(stamp("rig-a", item.id, "Creativity", 0.5, "Leaf", "rig-b"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn rig_remove_and_get_returns_none() {
        let board = new_board().await;
        board.register_rig("temp", "ai", None, None).await.unwrap();
        assert!(board.get_rig("temp").await.unwrap().is_some());
        board.remove_rig("temp").await.unwrap();
        assert!(board.get_rig("temp").await.unwrap().is_none());
    }

    // ── Task 7: Work item lifecycle tests ────────────────────────────

    #[tokio::test]
    async fn full_work_item_lifecycle() {
        let board = new_board().await;
        let item = board
            .post(PostWorkItem {
                title: "End to end".into(),
                description: "Full lifecycle test".into(),
                created_by: RigId::new("poster"),
                priority: Priority::P0,
                tags: vec!["integration".into()],
            })
            .await
            .unwrap();
        assert_eq!(item.status, Status::Open);

        let claimed = board.claim(item.id, &RigId::new("worker")).await.unwrap();
        assert_eq!(claimed.status, Status::Claimed);
        assert_eq!(claimed.claimed_by, Some(RigId::new("worker")));

        let done = board.submit(item.id, &RigId::new("worker")).await.unwrap();
        assert_eq!(done.status, Status::Done);

        let fetched = board.get(item.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, Status::Done);
        assert_eq!(fetched.priority, Priority::P0);
        assert_eq!(fetched.tags, vec!["integration"]);
    }

    #[tokio::test]
    async fn stuck_retry_lifecycle() {
        let board = new_board().await;
        let item = board.post(post_req("stuck test")).await.unwrap();
        board.claim(item.id, &RigId::new("worker")).await.unwrap();
        let stuck = board
            .mark_stuck(item.id, &RigId::new("worker"))
            .await
            .unwrap();
        assert_eq!(stuck.status, Status::Stuck);

        let retried = board.retry(item.id).await.unwrap();
        assert_eq!(retried.status, Status::Open);
        assert!(retried.claimed_by.is_none());
    }

    #[tokio::test]
    async fn claim_done_item_fails() {
        let board = new_board().await;
        let item = board.post(post_req("done item")).await.unwrap();
        board.claim(item.id, &RigId::new("w")).await.unwrap();
        board.submit(item.id, &RigId::new("w")).await.unwrap();

        let result = board.claim(item.id, &RigId::new("other")).await;
        assert!(result.is_err());
    }

    // ── Task 2: System rig tests ──────────────────────────────────

    #[tokio::test]
    async fn system_rigs_created_on_connect() {
        let board = Board::in_memory().await.unwrap();
        let human = board.get_rig("human").await.unwrap();
        assert!(human.is_some());
        assert_eq!(human.unwrap().rig_type, "system");

        let evolver = board.get_rig("evolver").await.unwrap();
        assert!(evolver.is_some());
        assert_eq!(evolver.unwrap().rig_type, "system");
    }

    #[tokio::test]
    async fn cannot_remove_system_rig() {
        let board = Board::in_memory().await.unwrap();
        let result = board.remove_rig("human").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn abandon_stuck_item() {
        let board = new_board().await;
        let item = board.post(post_req("abandon me")).await.unwrap();
        board.claim(item.id, &RigId::new("w")).await.unwrap();
        board.mark_stuck(item.id, &RigId::new("w")).await.unwrap();

        let abandoned = board.abandon(item.id).await.unwrap();
        assert_eq!(abandoned.status, Status::Abandoned);
    }

    // ── Task 3: stamp_notify + evolved_at ────────────────────────────

    #[tokio::test]
    async fn stamp_notify_fires_on_add_stamp() {
        let board = Board::in_memory().await.unwrap();
        let notify = board.stamp_notify_handle();
        let item = board.post(post_req("test")).await.unwrap();

        let handle = tokio::spawn(async move {
            notify.notified().await;
            true
        });

        tokio::task::yield_now().await;
        board
            .add_stamp(stamp("rig-a", item.id, "Quality", 0.5, "Leaf", "human"))
            .await
            .unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn unprocessed_low_stamps_returns_only_unevolved() {
        let board = Board::in_memory().await.unwrap();
        let item = board.post(post_req("test")).await.unwrap();

        let id1 = board
            .add_stamp(stamp("rig-a", item.id, "Quality", 0.2, "Leaf", "human"))
            .await
            .unwrap();
        let _id2 = board
            .add_stamp(stamp("rig-a", item.id, "Reliability", 0.8, "Leaf", "human"))
            .await
            .unwrap();

        let low = board.unprocessed_low_stamps(0.3).await.unwrap();
        assert_eq!(low.len(), 1);
        assert_eq!(low[0].id, id1);

        board.mark_stamp_evolved(id1).await.unwrap();
        let low = board.unprocessed_low_stamps(0.3).await.unwrap();
        assert!(low.is_empty());
    }

    // ── Task 1: claimed_by() tests ────────────────────────────────

    #[tokio::test]
    async fn claimed_by_returns_items_claimed_by_rig() {
        let board = new_board().await;
        let rig_a = RigId::new("worker-a");
        let rig_b = RigId::new("worker-b");

        board.post(post_req("task-1")).await.unwrap();
        board.post(post_req("task-2")).await.unwrap();
        board.post(post_req("task-3")).await.unwrap();

        board.claim(1, &rig_a).await.unwrap();
        board.claim(2, &rig_b).await.unwrap();
        board.claim(3, &rig_a).await.unwrap();

        let items = board.claimed_by(&rig_a).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[1].id, 3);

        let items_b = board.claimed_by(&rig_b).await.unwrap();
        assert_eq!(items_b.len(), 1);
        assert_eq!(items_b[0].id, 2);

        let empty = board.claimed_by(&RigId::new("nobody")).await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn claimed_by_sorts_by_priority_desc() {
        let board = new_board().await;
        let rig = RigId::new("worker");

        board.post(PostWorkItem {
            title: "low".to_string(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P2,
            tags: vec![],
        }).await.unwrap();
        board.post(PostWorkItem {
            title: "high".to_string(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P0,
            tags: vec![],
        }).await.unwrap();

        board.claim(1, &rig).await.unwrap();
        board.claim(2, &rig).await.unwrap();

        let items = board.claimed_by(&rig).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "high");  // P0 먼저
        assert_eq!(items[1].title, "low");   // P2 나중
    }
}
