// State transition operations for Board work items.

use crate::board::{Board, db_err};
use crate::entity;
use crate::work_item::{BoardError, PostWorkItem, RigId, Status, WorkItem};
use chrono::Utc;
use sea_orm::*;

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

        let item = self.get_or_err(result.last_insert_id).await?;
        // Sync to in-memory CowStore, then notify waiters
        self.store.lock().await.insert_to_main(item.clone());
        self.notify.notify_waiters();
        Ok(item)
    }

    pub async fn claim(&self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let txn = self.db.begin().await.map_err(db_err)?;

        let model = Self::find_model(&txn, item_id).await?;
        let current_status: Status = model.status;
        let current_claimed_by = model.claimed_by.clone();

        if current_status == Status::Claimed {
            return Err(BoardError::AlreadyClaimed {
                id: item_id,
                claimed_by: current_claimed_by
                    .map(RigId::new)
                    .unwrap_or_else(|| RigId::new("unknown")),
            });
        }
        current_status.validate_transition(Status::Claimed)?;

        let mut active: entity::work_item::ActiveModel = model.into();
        active.status = Set(Status::Claimed);
        active.claimed_by = Set(Some(rig_id.0.clone()));
        active.updated_at = Set(Utc::now());
        let updated = active.update(&txn).await.map_err(db_err)?;

        txn.commit().await.map_err(db_err)?;
        let result = WorkItem::from(updated);
        self.sync_item(&result).await;
        Ok(result)
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
}

#[cfg(test)]
mod tests {
    use crate::board::AddStampParams;
    use crate::test_helpers::{new_board, post_req};
    use crate::work_item::{PostWorkItem, Priority, RigId, Status};

    #[tokio::test]
    async fn post_creates_open_item() {
        let board = new_board().await;
        let item = board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        assert_eq!(item.id, 1);
        assert_eq!(item.status, Status::Open);
        assert!(item.claimed_by.is_none());
    }

    #[tokio::test]
    async fn auto_increment_ids() {
        let board = new_board().await;
        let a = board
            .post(post_req("a"))
            .await
            .expect("board post should succeed");
        let b = board
            .post(post_req("b"))
            .await
            .expect("board post should succeed");
        assert_eq!(a.id, 1);
        assert_eq!(b.id, 2);
    }

    #[tokio::test]
    async fn claim_and_submit() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");

        let rig = RigId::new("dev");
        let claimed = board.claim(1, &rig).await.expect("claim should succeed");
        assert_eq!(claimed.status, Status::Claimed);
        assert_eq!(claimed.claimed_by, Some(rig.clone()));

        let done = board.submit(1, &rig).await.expect("submit should succeed");
        assert_eq!(done.status, Status::Done);
    }

    #[tokio::test]
    async fn claim_already_claimed_fails() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        board
            .claim(1, &RigId::new("dev"))
            .await
            .expect("claim should succeed");
        assert!(board.claim(1, &RigId::new("other")).await.is_err());
    }

    #[tokio::test]
    async fn submit_wrong_rig_fails() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        board
            .claim(1, &RigId::new("dev"))
            .await
            .expect("claim should succeed");
        assert!(board.submit(1, &RigId::new("other")).await.is_err());
    }

    #[tokio::test]
    async fn unclaim_returns_to_open() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        let rig = RigId::new("dev");
        board.claim(1, &rig).await.expect("claim should succeed");

        let unclaimed = board
            .unclaim(1, &rig)
            .await
            .expect("async operation should succeed");
        assert_eq!(unclaimed.status, Status::Open);
        assert!(unclaimed.claimed_by.is_none());
    }

    #[tokio::test]
    async fn stuck_and_retry() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        let rig = RigId::new("dev");
        board.claim(1, &rig).await.expect("claim should succeed");

        board
            .mark_stuck(1, &rig)
            .await
            .expect("async operation should succeed");
        assert_eq!(
            board
                .get(1)
                .await
                .expect("get should succeed")
                .expect("operation should succeed")
                .status,
            Status::Stuck
        );

        board
            .retry(1)
            .await
            .expect("async operation should succeed");
        assert_eq!(
            board
                .get(1)
                .await
                .expect("get should succeed")
                .expect("operation should succeed")
                .status,
            Status::Open
        );
    }

    #[tokio::test]
    async fn abandon_from_open() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        board.abandon(1).await.expect("abandon should succeed");
        assert_eq!(
            board
                .get(1)
                .await
                .expect("get should succeed")
                .expect("operation should succeed")
                .status,
            Status::Abandoned
        );
    }

    #[tokio::test]
    async fn invalid_transition_fails() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        assert!(board.submit(1, &RigId::new("dev")).await.is_err());
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
            .expect("operation should succeed");
        assert_eq!(item.status, Status::Open);

        let claimed = board
            .claim(item.id, &RigId::new("worker"))
            .await
            .expect("claim should succeed");
        assert_eq!(claimed.status, Status::Claimed);
        assert_eq!(claimed.claimed_by, Some(RigId::new("worker")));

        let done = board
            .submit(item.id, &RigId::new("worker"))
            .await
            .expect("submit should succeed");
        assert_eq!(done.status, Status::Done);

        let fetched = board
            .get(item.id)
            .await
            .expect("get should succeed")
            .expect("operation should succeed");
        assert_eq!(fetched.status, Status::Done);
        assert_eq!(fetched.priority, Priority::P0);
        assert_eq!(fetched.tags, vec!["integration"]);
    }

    #[tokio::test]
    async fn stuck_retry_lifecycle() {
        let board = new_board().await;
        let item = board
            .post(post_req("stuck test"))
            .await
            .expect("board post should succeed");
        board
            .claim(item.id, &RigId::new("worker"))
            .await
            .expect("claim should succeed");
        let stuck = board
            .mark_stuck(item.id, &RigId::new("worker"))
            .await
            .expect("operation should succeed");
        assert_eq!(stuck.status, Status::Stuck);

        let retried = board
            .retry(item.id)
            .await
            .expect("async operation should succeed");
        assert_eq!(retried.status, Status::Open);
        assert!(retried.claimed_by.is_none());
    }

    #[tokio::test]
    async fn claim_done_item_fails() {
        let board = new_board().await;
        let item = board
            .post(post_req("done item"))
            .await
            .expect("board post should succeed");
        board
            .claim(item.id, &RigId::new("w"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("w"))
            .await
            .expect("submit should succeed");

        let result = board.claim(item.id, &RigId::new("other")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn abandon_stuck_item() {
        let board = new_board().await;
        let item = board
            .post(post_req("abandon me"))
            .await
            .expect("board post should succeed");
        board
            .claim(item.id, &RigId::new("w"))
            .await
            .expect("claim should succeed");
        board
            .mark_stuck(item.id, &RigId::new("w"))
            .await
            .expect("async operation should succeed");

        let abandoned = board
            .abandon(item.id)
            .await
            .expect("abandon should succeed");
        assert_eq!(abandoned.status, Status::Abandoned);
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
        board
            .post(post_req("wake"))
            .await
            .expect("board post should succeed");

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn claim_nonexistent_item_fails() {
        let board = new_board().await;
        let result = board.claim(999, &RigId::new("dev")).await;
        assert!(
            matches!(result, Err(crate::work_item::BoardError::NotFound(999))),
            "expected NotFound(999), got {result:?}"
        );
    }

    #[tokio::test]
    async fn submit_nonexistent_item_fails() {
        let board = new_board().await;
        let result = board.submit(999, &RigId::new("dev")).await;
        assert!(
            matches!(result, Err(crate::work_item::BoardError::NotFound(999))),
            "expected NotFound(999), got {result:?}"
        );
    }

    #[tokio::test]
    async fn done_to_claimed_returns_invalid_transition() {
        let board = new_board().await;
        let item = board
            .post(post_req("terminal"))
            .await
            .expect("board post should succeed");
        board
            .claim(item.id, &RigId::new("w"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("w"))
            .await
            .expect("submit should succeed");

        let result = board.claim(item.id, &RigId::new("other")).await;
        assert!(
            matches!(
                result,
                Err(crate::work_item::BoardError::InvalidTransition(_))
            ),
            "expected InvalidTransition, got {result:?}"
        );
    }

    #[tokio::test]
    async fn unclaim_wrong_rig_fails() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        board
            .claim(1, &RigId::new("dev"))
            .await
            .expect("claim should succeed");

        let result = board.unclaim(1, &RigId::new("other")).await;
        assert!(
            matches!(
                result,
                Err(crate::work_item::BoardError::NotClaimedBy { .. })
            ),
            "expected NotClaimedBy, got {result:?}"
        );
    }

    #[tokio::test]
    async fn mark_stuck_wrong_rig_fails() {
        let board = new_board().await;
        board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");
        board
            .claim(1, &RigId::new("dev"))
            .await
            .expect("claim should succeed");

        let result = board.mark_stuck(1, &RigId::new("other")).await;
        assert!(
            matches!(
                result,
                Err(crate::work_item::BoardError::NotClaimedBy { .. })
            ),
            "expected NotClaimedBy, got {result:?}"
        );
    }

    #[tokio::test]
    async fn abandon_done_item_fails() {
        let board = new_board().await;
        let item = board
            .post(post_req("done item"))
            .await
            .expect("board post should succeed");
        board
            .claim(item.id, &RigId::new("w"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("w"))
            .await
            .expect("submit should succeed");

        let result = board.abandon(item.id).await;
        assert!(
            matches!(
                result,
                Err(crate::work_item::BoardError::InvalidTransition(_))
            ),
            "expected InvalidTransition for Done->Abandoned, got {result:?}"
        );
    }

    #[tokio::test]
    async fn submit_unclaimed_item_fails() {
        let board = new_board().await;
        board
            .post(post_req("open item"))
            .await
            .expect("board post should succeed");

        let result = board.submit(1, &RigId::new("dev")).await;
        assert!(
            result.is_err(),
            "submit on Open item should fail"
        );
    }

    #[tokio::test]
    async fn stamp_notify_fires_on_add_stamp() {
        let board = crate::board::Board::in_memory()
            .await
            .expect("in-memory board should initialize");
        let notify = board.stamp_notify_handle();
        let item = board
            .post(post_req("test"))
            .await
            .expect("board post should succeed");

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
            .expect("operation should succeed");

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;
        assert!(result.is_ok());
    }
}
