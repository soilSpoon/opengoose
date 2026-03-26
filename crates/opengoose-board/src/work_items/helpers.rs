// Internal helper utilities for Board work items.

use crate::board::{Board, db_err};
use crate::entity;
use crate::work_item::{BoardError, Status, WorkItem};
use chrono::Utc;
use sea_orm::*;
use std::future::Future;
use std::pin::Pin;

impl Board {
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
        let result = WorkItem::from(updated);
        self.sync_item(&result).await;
        Ok(result)
    }

    /// CowStore에 아이템을 동기화. 모든 상태 변경 후 호출.
    pub(crate) async fn sync_item(&self, item: &WorkItem) {
        let synced = item.clone();
        self.store.write().await.update_in_main(item.id, |i| {
            *i = synced;
        });
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
    /// - summarizer: description → 요약 생성 콜백
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

            // 트랜잭션 내에서 status + description 재확인
            let txn = self.db.begin().await.map_err(db_err)?;
            let fresh = Self::find_model(&txn, model.id).await?;
            if !matches!(
                fresh.status,
                Status::Done | Status::Abandoned | Status::Stuck
            ) {
                txn.rollback().await.map_err(db_err)?;
                continue;
            }
            if fresh.description.is_empty() {
                txn.rollback().await.map_err(db_err)?;
                continue;
            }

            // fresh description으로 요약 생성 — stale-write 방지
            let summary = summarizer(&fresh.description).await?;

            let item_id = fresh.id;
            let mut active: entity::work_item::ActiveModel = fresh.into();
            active.description = Set(summary.clone());
            active.updated_at = Set(Utc::now());
            active.update(&txn).await.map_err(db_err)?;
            txn.commit().await.map_err(db_err)?;

            // CowStore 동기화
            self.store.write().await.update_in_main(item_id, |item| {
                item.description = summary.clone();
                item.updated_at = Utc::now();
            });

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

        Ok(relations
            .iter()
            .filter(|rel| !done_blockers.contains(&rel.from_id))
            .map(|rel| rel.to_id)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::board::{AddStampParams, Board};
    use crate::entity;
    use crate::test_helpers::{new_board, post_req};
    use crate::work_item::{BoardError, PostWorkItem, Priority, RigId, Status};
    use chrono::Utc;
    use sea_orm::*;

    /// Test-only: backdate an item's updated_at by N days.
    async fn backdate_for_test(board: &Board, item_id: i64, days: i64) {
        let model = entity::work_item::Entity::find_by_id(item_id)
            .one(&board.db)
            .await
            .expect("find_by_id query should succeed")
            .expect("work item should exist");
        let mut active: entity::work_item::ActiveModel = model.into();
        active.updated_at = Set(Utc::now() - chrono::Duration::days(days));
        active
            .update(&board.db)
            .await
            .expect("async operation should succeed");
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
            .expect("board operation should succeed");
        board
            .claim(item.id, &RigId::new("w"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("w"))
            .await
            .expect("submit should succeed");

        // Backdate updated_at to 31 days ago
        backdate_for_test(&board, item.id, 31).await;

        // Run compact with a simple summarizer
        let count = board
            .compact(chrono::Duration::days(30), |desc: &str| {
                Box::pin(async move { Ok(format!("[summary] {}", &desc[..20])) })
            })
            .await
            .expect("board operation should succeed");

        assert_eq!(count, 1);
        let compacted = board
            .get(item.id)
            .await
            .expect("get should succeed")
            .expect("item should exist");
        assert!(compacted.description.starts_with("[summary]"));
    }

    #[tokio::test]
    async fn compact_skips_recent_items() {
        let board = new_board().await;
        let item = board
            .post(post_req("recent"))
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

        // Don't backdate — item is recent
        let count = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("should not be called".into()) })
            })
            .await
            .expect("board operation should succeed");

        assert_eq!(count, 0);
        let fetched = board
            .get(item.id)
            .await
            .expect("get should succeed")
            .expect("item should exist");
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
            .expect("board operation should succeed");

        backdate_for_test(&board, item.id, 60).await;

        let count = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("compacted".into()) })
            })
            .await
            .expect("board operation should succeed");

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
            .expect("board operation should succeed");
        let item_b = board
            .post(PostWorkItem {
                title: "blocked".into(),
                description: "Detailed description to be compacted".into(),
                created_by: RigId::new("user"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("board operation should succeed");

        board
            .add_dependency(item_a.id, item_b.id)
            .await
            .expect("async operation should succeed");
        board
            .claim(item_a.id, &RigId::new("w"))
            .await
            .expect("claim should succeed");
        board
            .submit(item_a.id, &RigId::new("w"))
            .await
            .expect("submit should succeed");

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
            .expect("board operation should succeed");

        backdate_for_test(&board, item_a.id, 60).await;

        let count = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("compacted summary".into()) })
            })
            .await
            .expect("board operation should succeed");

        assert_eq!(count, 1);

        // Stamps still exist
        let stamps = board
            .stamps_for_item(item_a.id)
            .await
            .expect("async operation should succeed");
        assert_eq!(stamps.len(), 1);
        assert_eq!(stamps[0].dimension, "Quality");

        // Item metadata preserved
        let compacted = board
            .get(item_a.id)
            .await
            .expect("get should succeed")
            .expect("item should exist");
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
            .expect("board operation should succeed");
        board
            .claim(item.id, &RigId::new("w"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("w"))
            .await
            .expect("submit should succeed");
        backdate_for_test(&board, item.id, 60).await;

        let result = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Err(BoardError::DbError("LLM unavailable".into())) })
            })
            .await;

        result.unwrap_err();
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
            .expect("board operation should succeed");
        board
            .claim(item.id, &RigId::new("w"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("w"))
            .await
            .expect("submit should succeed");
        backdate_for_test(&board, item.id, 60).await;

        // First run: compacts
        let count1 = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("summary".into()) })
            })
            .await
            .expect("board operation should succeed");
        assert_eq!(count1, 1);

        // Second run: updated_at was refreshed, so item is now "recent"
        let count2 = board
            .compact(chrono::Duration::days(30), |_: &str| {
                Box::pin(async { Ok("re-summarized".into()) })
            })
            .await
            .expect("board operation should succeed");
        assert_eq!(count2, 0);

        // Description stays as first summary
        let item = board
            .get(item.id)
            .await
            .expect("get should succeed")
            .expect("item should exist");
        assert_eq!(item.description, "summary");
    }
}
