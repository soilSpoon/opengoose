// DB Board — SQLite 기반 Wanted Board
//
// SeaORM + SQLite. 모든 메서드가 async.

use crate::entity;
use crate::work_item::{BoardError, PostWorkItem, RigId, Status, WorkItem};
use chrono::Utc;
use sea_orm::*;
use std::sync::Arc;
use tokio::sync::Notify;

pub struct DbBoard {
    db: DatabaseConnection,
    notify: Arc<Notify>,
}

impl DbBoard {
    pub async fn connect(db_url: &str) -> Result<Self, BoardError> {
        let db = Database::connect(db_url).await.map_err(db_err)?;
        Self::create_tables(&db).await?;
        Ok(Self {
            db,
            notify: Arc::new(Notify::new()),
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

    // ── 기본 API ─────────────────────────────────────────────

    pub async fn post(&self, req: PostWorkItem) -> Result<WorkItem, BoardError> {
        let now = Utc::now();
        let tags_json = if req.tags.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&req.tags).unwrap_or_default())
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
        let item = self.get_or_err(item_id).await?;

        if item.status == Status::Claimed {
            return Err(BoardError::AlreadyClaimed {
                id: item_id,
                claimed_by: item.claimed_by.unwrap_or_else(|| RigId::new("unknown")),
            });
        }
        item.status.validate_transition(Status::Claimed)?;

        self.update_item(item_id, |mut m| {
            m.status = Set(Status::Claimed);
            m.claimed_by = Set(Some(rig_id.0.clone()));
            m.updated_at = Set(Utc::now());
            m
        })
        .await
    }

    pub async fn submit(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let item = self.get_or_err(item_id).await?;
        Self::verify_claimed_by(&item, rig_id)?;
        item.status.validate_transition(Status::Done)?;

        self.update_item(item_id, |mut m| {
            m.status = Set(Status::Done);
            m.updated_at = Set(Utc::now());
            m
        })
        .await
    }

    pub async fn unclaim(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let item = self.get_or_err(item_id).await?;
        Self::verify_claimed_by(&item, rig_id)?;
        item.status.validate_transition(Status::Open)?;

        let result = self
            .update_item(item_id, |mut m| {
                m.status = Set(Status::Open);
                m.claimed_by = Set(None);
                m.updated_at = Set(Utc::now());
                m
            })
            .await?;

        self.notify.notify_waiters();
        Ok(result)
    }

    pub async fn mark_stuck(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let item = self.get_or_err(item_id).await?;

        if let Some(ref claimed) = item.claimed_by
            && claimed != rig_id
        {
            return Err(BoardError::NotClaimedBy {
                id: item_id,
                claimed_by: claimed.clone(),
                attempted_by: rig_id.clone(),
            });
        }
        item.status.validate_transition(Status::Stuck)?;

        self.update_item(item_id, |mut m| {
            m.status = Set(Status::Stuck);
            m.updated_at = Set(Utc::now());
            m
        })
        .await
    }

    pub async fn retry(&self, item_id: i64) -> Result<WorkItem, BoardError> {
        let item = self.get_or_err(item_id).await?;
        item.status.validate_transition(Status::Open)?;

        let result = self
            .update_item(item_id, |mut m| {
                m.status = Set(Status::Open);
                m.claimed_by = Set(None);
                m.updated_at = Set(Utc::now());
                m
            })
            .await?;

        self.notify.notify_waiters();
        Ok(result)
    }

    pub async fn abandon(&self, item_id: i64) -> Result<WorkItem, BoardError> {
        let item = self.get_or_err(item_id).await?;
        item.status.validate_transition(Status::Abandoned)?;

        self.update_item(item_id, |mut m| {
            m.status = Set(Status::Abandoned);
            m.updated_at = Set(Utc::now());
            m
        })
        .await
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

    // ── Rigs ──────────────────────────────────────────────────

    pub async fn register_rig(
        &self,
        id: &str,
        rig_type: &str,
        recipe: Option<&str>,
        tags: Option<&[String]>,
    ) -> Result<(), BoardError> {
        let tags_json = tags.map(|t| serde_json::to_string(t).unwrap_or_default());

        let model = entity::rig::ActiveModel {
            id: Set(id.to_string()),
            rig_type: Set(rig_type.to_string()),
            recipe: Set(recipe.map(|s| s.to_string())),
            tags: Set(tags_json),
            created_at: Set(chrono::Utc::now()),
        };

        // upsert: 이미 있으면 무시 (멱등)
        match entity::rig::Entity::insert(model).exec(&self.db).await {
            Ok(_) => Ok(()),
            Err(DbErr::Exec(sea_orm::RuntimeErr::SqlxError(e)))
                if e.to_string().contains("UNIQUE") =>
            {
                Ok(()) // 이미 등록됨
            }
            Err(e) => Err(db_err(e)),
        }
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

    pub async fn add_stamp(
        &self,
        target_rig: &str,
        work_item_id: i64,
        dimension: &str,
        score: f32,
        severity: &str,
        stamped_by: &str,
    ) -> Result<(), BoardError> {
        // 졸업앨범 규칙
        if stamped_by == target_rig {
            return Err(BoardError::YearbookViolation {
                stamper: RigId::new(stamped_by),
                target: RigId::new(target_rig),
            });
        }
        // score 범위
        if !(-1.0..=1.0).contains(&score) {
            return Err(BoardError::InvalidScore(score));
        }

        entity::stamp::Entity::insert(entity::stamp::ActiveModel {
            id: NotSet,
            target_rig: Set(target_rig.to_string()),
            work_item_id: Set(work_item_id),
            dimension: Set(dimension.to_string()),
            score: Set(score),
            severity: Set(severity.to_string()),
            stamped_by: Set(stamped_by.to_string()),
            timestamp: Set(chrono::Utc::now()),
        })
        .exec(&self.db)
        .await
        .map_err(db_err)?;

        Ok(())
    }

    /// 가중 점수 (시간 감쇠 적용). 30일 반감기.
    pub async fn weighted_score(&self, rig_id: &str) -> Result<f32, BoardError> {
        let stamps = entity::stamp::Entity::find()
            .filter(entity::stamp::Column::TargetRig.eq(rig_id))
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let now = chrono::Utc::now();
        let score = stamps.iter().map(|s| {
            let days = (now - s.timestamp).num_seconds() as f32 / 86400.0;
            let decay = 0.5_f32.powf(days / 30.0);
            let severity_weight = match s.severity.as_str() {
                "Root" => 4.0,
                "Branch" => 2.0,
                _ => 1.0, // Leaf
            };
            severity_weight * s.score * decay
        }).sum();

        Ok(score)
    }

    /// 신뢰 수준.
    pub async fn trust_level(&self, rig_id: &str) -> Result<&'static str, BoardError> {
        let score = self.weighted_score(rig_id).await?;
        Ok(if score >= 50.0 { "L3" }
        else if score >= 25.0 { "L2.5" }
        else if score >= 10.0 { "L2" }
        else if score >= 3.0 { "L1.5" }
        else { "L1" })
    }

    // ── 알림 ─────────────────────────────────────────────────

    pub async fn wait_for_claimable(&self) {
        self.notify.notified().await;
    }

    pub fn notify_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    // ── 내부 헬퍼 ────────────────────────────────────────────

    async fn get_or_err(&self, item_id: i64) -> Result<WorkItem, BoardError> {
        self.get(item_id)
            .await?
            .ok_or(BoardError::NotFound(item_id))
    }

    /// find → ActiveModel 변환 → 클로저로 수정 → update → 도메인 타입 반환.
    async fn update_item(
        &self,
        item_id: i64,
        f: impl FnOnce(entity::work_item::ActiveModel) -> entity::work_item::ActiveModel,
    ) -> Result<WorkItem, BoardError> {
        let model = entity::work_item::Entity::find_by_id(item_id)
            .one(&self.db)
            .await
            .map_err(db_err)?
            .ok_or(BoardError::NotFound(item_id))?;

        let active: entity::work_item::ActiveModel = model.into();
        let updated = f(active).update(&self.db).await.map_err(db_err)?;
        Ok(WorkItem::from(updated))
    }

    fn verify_claimed_by(item: &WorkItem, rig_id: &RigId) -> Result<(), BoardError> {
        match &item.claimed_by {
            Some(claimed) if claimed != rig_id => Err(BoardError::NotClaimedBy {
                id: item.id,
                claimed_by: claimed.clone(),
                attempted_by: rig_id.clone(),
            }),
            None => Err(BoardError::NotClaimed { id: item.id }),
            _ => Ok(()),
        }
    }

    async fn blocked_item_ids(&self) -> Result<std::collections::HashSet<i64>, BoardError> {
        let relations = entity::relation::Entity::find()
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let mut blocked = std::collections::HashSet::new();
        for rel in &relations {
            let blocker_done = entity::work_item::Entity::find_by_id(rel.from_id)
                .one(&self.db)
                .await
                .map_err(db_err)?
                .map(|m| m.status == Status::Done)
                .unwrap_or(false);

            if !blocker_done {
                blocked.insert(rel.to_id);
            }
        }
        Ok(blocked)
    }

    async fn would_create_cycle(&self, from: i64, to: i64) -> Result<bool, BoardError> {
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(from);

        while let Some(current) = queue.pop_front() {
            if current == to {
                return Ok(true);
            }
            if visited.insert(current) {
                let blockers = entity::relation::Entity::find()
                    .filter(entity::relation::Column::ToId.eq(current))
                    .all(&self.db)
                    .await
                    .map_err(db_err)?;

                for rel in blockers {
                    if !visited.contains(&rel.from_id) {
                        queue.push_back(rel.from_id);
                    }
                }
            }
        }
        Ok(false)
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

    async fn new_board() -> DbBoard {
        DbBoard::in_memory().await.unwrap()
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
        assert_eq!(board.get(1).await.unwrap().unwrap().status, Status::Abandoned);
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
}
