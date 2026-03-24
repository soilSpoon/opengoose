// Read-only query operations for Board work items.

use crate::board::{Board, db_err};
use crate::entity;
use crate::work_item::{BoardError, RigId, Status, WorkItem};
use sea_orm::*;

impl Board {
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

        items.sort_by_key(|item| std::cmp::Reverse(item.priority.urgency()));
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

        items.sort_by_key(|item| std::cmp::Reverse(item.priority.urgency()));
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
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::{new_board, post_req};
    use crate::work_item::{PostWorkItem, Priority, RigId};

    #[tokio::test]
    async fn ready_excludes_blocked() {
        let board = new_board().await;
        let a = board
            .post(post_req("blocker"))
            .await
            .expect("board post should succeed");
        let b = board
            .post(post_req("blocked"))
            .await
            .expect("board post should succeed");
        board
            .add_dependency(a.id, b.id)
            .await
            .expect("async operation should succeed");

        let ids: Vec<i64> = board
            .ready()
            .await
            .expect("async operation should succeed")
            .iter()
            .map(|i| i.id)
            .collect();
        assert!(ids.contains(&a.id));
        assert!(!ids.contains(&b.id));
    }

    #[tokio::test]
    async fn ready_unblocks_when_done() {
        let board = new_board().await;
        let a = board
            .post(post_req("blocker"))
            .await
            .expect("board post should succeed");
        let b = board
            .post(post_req("blocked"))
            .await
            .expect("board post should succeed");
        board
            .add_dependency(a.id, b.id)
            .await
            .expect("async operation should succeed");

        board
            .claim(a.id, &RigId::new("dev"))
            .await
            .expect("claim should succeed");
        board
            .submit(a.id, &RigId::new("dev"))
            .await
            .expect("submit should succeed");

        let ids: Vec<i64> = board
            .ready()
            .await
            .expect("async operation should succeed")
            .iter()
            .map(|i| i.id)
            .collect();
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
            .expect("operation should succeed");
        board
            .post(PostWorkItem {
                title: "urgent".into(),
                description: String::new(),
                created_by: RigId::new("user"),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .expect("operation should succeed");

        let ready = board.ready().await.expect("async operation should succeed");
        assert_eq!(ready[0].priority, Priority::P0);
        assert_eq!(ready[1].priority, Priority::P2);
    }

    #[tokio::test]
    async fn list_returns_all() {
        let board = new_board().await;
        board
            .post(post_req("a"))
            .await
            .expect("board post should succeed");
        board
            .post(post_req("b"))
            .await
            .expect("board post should succeed");
        assert_eq!(board.list().await.expect("list should succeed").len(), 2);
    }

    #[tokio::test]
    async fn claimed_by_returns_items_claimed_by_rig() {
        let board = new_board().await;
        let rig_a = RigId::new("worker-a");
        let rig_b = RigId::new("worker-b");

        board
            .post(post_req("task-1"))
            .await
            .expect("board post should succeed");
        board
            .post(post_req("task-2"))
            .await
            .expect("board post should succeed");
        board
            .post(post_req("task-3"))
            .await
            .expect("board post should succeed");

        board.claim(1, &rig_a).await.expect("claim should succeed");
        board.claim(2, &rig_b).await.expect("claim should succeed");
        board.claim(3, &rig_a).await.expect("claim should succeed");

        let items = board
            .claimed_by(&rig_a)
            .await
            .expect("async operation should succeed");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[1].id, 3);

        let items_b = board
            .claimed_by(&rig_b)
            .await
            .expect("async operation should succeed");
        assert_eq!(items_b.len(), 1);
        assert_eq!(items_b[0].id, 2);

        let empty = board
            .claimed_by(&RigId::new("nobody"))
            .await
            .expect("async operation should succeed");
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown_id() {
        let board = new_board().await;
        let result = board.get(999).await.expect("get should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn completed_by_rig_returns_only_submitted() {
        let board = new_board().await;
        let rig = RigId::new("worker");

        let item1 = board
            .post(post_req("done-task"))
            .await
            .expect("board post should succeed");
        let item2 = board
            .post(post_req("claimed-task"))
            .await
            .expect("board post should succeed");
        let item3 = board
            .post(post_req("other-done"))
            .await
            .expect("board post should succeed");

        // item1: claim + submit (done)
        board.claim(item1.id, &rig).await.expect("claim should succeed");
        board.submit(item1.id, &rig).await.expect("submit should succeed");

        // item2: only claimed (not done)
        board.claim(item2.id, &rig).await.expect("claim should succeed");

        // item3: done by different rig
        let other = RigId::new("other");
        board.claim(item3.id, &other).await.expect("claim should succeed");
        board.submit(item3.id, &other).await.expect("submit should succeed");

        let completed = board
            .completed_by_rig("worker")
            .await
            .expect("completed_by_rig should succeed");
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, item1.id);
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
            .expect("operation should succeed");
        board
            .post(PostWorkItem {
                title: "high".to_string(),
                description: String::new(),
                created_by: RigId::new("user"),
                priority: Priority::P0,
                tags: vec![],
            })
            .await
            .expect("operation should succeed");

        board.claim(1, &rig).await.expect("claim should succeed");
        board.claim(2, &rig).await.expect("claim should succeed");

        let items = board
            .claimed_by(&rig)
            .await
            .expect("async operation should succeed");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "high"); // P0 먼저
        assert_eq!(items[1].title, "low"); // P2 나중
    }
}
