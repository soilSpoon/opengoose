// Work item CRUD operations for Board.

use crate::board::Board;
use crate::entity;
use crate::work_item::{BoardError, PostWorkItem, RigId, Status, WorkItem};
use chrono::Utc;
use sea_orm::*;
use std::future::Future;
use std::pin::Pin;

fn db_err(e: DbErr) -> BoardError {
    BoardError::DbError(e.to_string())
}

impl Board {
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

    // ── 내부 헬퍼 ────────────────────────────────────────────

    /// 공통 상태 전이 패턴. txn begin → find → validate → apply → commit.
    pub(crate) async fn transition(
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

    pub(crate) async fn get_or_err(&self, item_id: i64) -> Result<WorkItem, BoardError> {
        self.get(item_id)
            .await?
            .ok_or(BoardError::NotFound(item_id))
    }

    /// 트랜잭션 내부용: Model을 직접 반환 (ActiveModel 변환에 사용).
    pub(crate) async fn find_model<C: ConnectionTrait>(
        conn: &C,
        item_id: i64,
    ) -> Result<entity::work_item::Model, BoardError> {
        entity::work_item::Entity::find_by_id(item_id)
            .one(conn)
            .await
            .map_err(db_err)?
            .ok_or(BoardError::NotFound(item_id))
    }

    /// compact() — 오래된 닫힌 항목의 description을 요약으로 교체.
    ///
    /// - older_than: 이 기간보다 오래된 항목만 대상
    /// - summarizer: description → 요약 생성 콜백 (LLM 호출 등)
    ///
    /// 보존: id, title, status, stamps, relations, created_at
    /// 교체: description
    ///
    /// Returns: 압축된 항목 수
    pub async fn compact<F>(
        &self,
        older_than: chrono::Duration,
        summarizer: F,
    ) -> Result<usize, BoardError>
    where
        F: Fn(&str) -> Pin<Box<dyn Future<Output = Result<String, BoardError>> + Send + '_>>,
    {
        let now = Utc::now();
        let cutoff = now - older_than;

        // 닫힌 상태 + 오래된 항목 조회
        let closed_statuses = vec![
            Status::Done.to_value(),
            Status::Abandoned.to_value(),
            Status::Stuck.to_value(),
        ];
        let models = entity::work_item::Entity::find()
            .filter(entity::work_item::Column::Status.is_in(closed_statuses))
            .filter(entity::work_item::Column::UpdatedAt.lt(cutoff))
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let mut count = 0;
        for model in models {
            if model.description.is_empty() {
                continue;
            }

            let summary = summarizer(&model.description).await?;

            let mut active: entity::work_item::ActiveModel = model.into();
            active.description = Set(summary);
            active.updated_at = Set(Utc::now());
            active.update(&self.db).await.map_err(db_err)?;
            count += 1;
        }

        Ok(count)
    }

    /// 블록된 아이템 ID 집합. 단일 배치 쿼리로 블로커 상태를 확인.
    pub(crate) async fn blocked_item_ids(
        &self,
    ) -> Result<std::collections::HashSet<i64>, BoardError> {
        let relations = entity::relation::Entity::find()
            .all(&self.db)
            .await
            .map_err(db_err)?;

        if relations.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::AddStampParams;
    use crate::work_item::Priority;

    /// Test-only: backdate an item's updated_at by N days.
    async fn backdate_for_test(board: &Board, item_id: i64, days: i64) {
        let model = entity::work_item::Entity::find_by_id(item_id)
            .one(&board.db)
            .await
            .unwrap()
            .unwrap();
        let mut active: entity::work_item::ActiveModel = model.into();
        active.updated_at = Set(Utc::now() - chrono::Duration::days(days));
        active.update(&board.db).await.unwrap();
    }

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
    async fn list_returns_all() {
        let board = new_board().await;
        board.post(post_req("a")).await.unwrap();
        board.post(post_req("b")).await.unwrap();
        assert_eq!(board.list().await.unwrap().len(), 2);
    }

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

    #[tokio::test]
    async fn abandon_stuck_item() {
        let board = new_board().await;
        let item = board.post(post_req("abandon me")).await.unwrap();
        board.claim(item.id, &RigId::new("w")).await.unwrap();
        board.mark_stuck(item.id, &RigId::new("w")).await.unwrap();

        let abandoned = board.abandon(item.id).await.unwrap();
        assert_eq!(abandoned.status, Status::Abandoned);
    }

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

        board
            .post(PostWorkItem {
                title: "low".to_string(),
                description: String::new(),
                created_by: RigId::new("user"),
                priority: Priority::P2,
                tags: vec![],
            })
            .await
            .unwrap();
        board
            .post(PostWorkItem {
                title: "high".to_string(),
                description: String::new(),
                created_by: RigId::new("user"),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .unwrap();

        board.claim(1, &rig).await.unwrap();
        board.claim(2, &rig).await.unwrap();

        let items = board.claimed_by(&rig).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "high"); // P0 먼저
        assert_eq!(items[1].title, "low"); // P2 나중
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
            .add_stamp(AddStampParams {
                target_rig: "rig-a",
                work_item_id: item.id,
                dimension: "Quality",
                score: 0.5,
                severity: "Leaf",
                stamped_by: "human",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn compact_replaces_old_done_descriptions() {
        let board = new_board().await;

        let item = board
            .post(PostWorkItem {
                title: "old task".into(),
                description: "Very long detailed description that should be compacted".into(),
                created_by: RigId::new("user"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board.claim(item.id, &RigId::new("w")).await.unwrap();
        board.submit(item.id, &RigId::new("w")).await.unwrap();

        // Backdate updated_at to 31 days ago
        backdate_for_test(&board, item.id, 31).await;

        // Run compact with a simple summarizer
        let count = board
            .compact(chrono::Duration::days(30), |desc: &str| {
                Box::pin(async move {
                    Ok(format!("[summary] {}", &desc[..20]))
                })
            })
            .await
            .unwrap();

        assert_eq!(count, 1);
        let compacted = board.get(item.id).await.unwrap().unwrap();
        assert!(compacted.description.starts_with("[summary]"));
    }

    #[tokio::test]
    async fn compact_skips_recent_items() {
        let board = new_board().await;
        let item = board.post(post_req("recent")).await.unwrap();
        board.claim(item.id, &RigId::new("w")).await.unwrap();
        board.submit(item.id, &RigId::new("w")).await.unwrap();

        // Don't backdate — item is recent
        let count = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("should not be called".into()) })
            })
            .await
            .unwrap();

        assert_eq!(count, 0);
        let fetched = board.get(item.id).await.unwrap().unwrap();
        assert!(!fetched.description.contains("should not be called"));
    }

    #[tokio::test]
    async fn compact_skips_open_items() {
        let board = new_board().await;
        let item = board
            .post(PostWorkItem {
                title: "open item".into(),
                description: "This is open and should not be compacted".into(),
                created_by: RigId::new("user"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        backdate_for_test(&board, item.id, 60).await;

        let count = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("compacted".into()) })
            })
            .await
            .unwrap();

        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn compact_preserves_stamps_and_relations() {
        let board = new_board().await;

        let item_a = board
            .post(PostWorkItem {
                title: "blocker".into(),
                description: "Blocker description to be compacted".into(),
                created_by: RigId::new("user"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        let item_b = board
            .post(PostWorkItem {
                title: "blocked".into(),
                description: "Detailed description to be compacted".into(),
                created_by: RigId::new("user"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();

        board.add_dependency(item_a.id, item_b.id).await.unwrap();
        board.claim(item_a.id, &RigId::new("w")).await.unwrap();
        board.submit(item_a.id, &RigId::new("w")).await.unwrap();

        board
            .add_stamp(AddStampParams {
                target_rig: "w",
                work_item_id: item_a.id,
                dimension: "Quality",
                score: 0.8,
                severity: "Leaf",
                stamped_by: "human",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .unwrap();

        backdate_for_test(&board, item_a.id, 60).await;

        let count = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("compacted summary".into()) })
            })
            .await
            .unwrap();

        assert_eq!(count, 1);

        // Stamps still exist
        let stamps = board.stamps_for_item(item_a.id).await.unwrap();
        assert_eq!(stamps.len(), 1);
        assert_eq!(stamps[0].dimension, "Quality");

        // Item metadata preserved
        let compacted = board.get(item_a.id).await.unwrap().unwrap();
        assert_eq!(compacted.title, "blocker");
        assert_eq!(compacted.status, Status::Done);
        assert_eq!(compacted.description, "compacted summary");
    }

    #[tokio::test]
    async fn compact_propagates_summarizer_error() {
        let board = new_board().await;
        let item = board
            .post(PostWorkItem {
                title: "will fail".into(),
                description: "Some description".into(),
                created_by: RigId::new("user"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board.claim(item.id, &RigId::new("w")).await.unwrap();
        board.submit(item.id, &RigId::new("w")).await.unwrap();
        backdate_for_test(&board, item.id, 60).await;

        let result = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Err(BoardError::DbError("LLM unavailable".into())) })
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn compact_is_idempotent() {
        let board = new_board().await;
        let item = board
            .post(PostWorkItem {
                title: "task".into(),
                description: "Long description to compact".into(),
                created_by: RigId::new("user"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .unwrap();
        board.claim(item.id, &RigId::new("w")).await.unwrap();
        board.submit(item.id, &RigId::new("w")).await.unwrap();
        backdate_for_test(&board, item.id, 60).await;

        // First run: compacts
        let count1 = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("summary".into()) })
            })
            .await
            .unwrap();
        assert_eq!(count1, 1);

        // Second run: updated_at was refreshed, so item is now "recent"
        let count2 = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("re-summarized".into()) })
            })
            .await
            .unwrap();
        assert_eq!(count2, 0);

        // Description stays as first summary
        let item = board.get(item.id).await.unwrap().unwrap();
        assert_eq!(item.description, "summary");
    }
}
