// Rig registration and lifecycle operations for Board.

use crate::board::{Board, db_err};
use crate::entity;
use crate::work_item::BoardError;
use sea_orm::*;

impl Board {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{new_board, post_req};
    use crate::work_item::RigId;

    #[tokio::test]
    async fn rig_lifecycle_register_stamp_trust() {
        let board = new_board().await;
        board
            .register_rig("ai-01", "ai", Some("developer"), Some(&["rust".into()]))
            .await
            .expect("operation should succeed");
        let rig = board
            .get_rig("ai-01")
            .await
            .expect("async operation should succeed")
            .expect("operation should succeed");
        assert_eq!(rig.rig_type, "ai");

        let level = board
            .trust_level("ai-01")
            .await
            .expect("async operation should succeed");
        assert_eq!(level, "L1");

        let item = board
            .post(post_req("task 1"))
            .await
            .expect("board post should succeed");
        board
            .claim(item.id, &RigId::new("ai-01"))
            .await
            .expect("claim should succeed");
        board
            .submit(item.id, &RigId::new("ai-01"))
            .await
            .expect("submit should succeed");

        board
            .add_stamp(crate::board::AddStampParams {
                target_rig: "ai-01",
                work_item_id: item.id,
                dimension: "Quality",
                score: 1.0,
                severity: "Root",
                stamped_by: "reviewer",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .expect("operation should succeed");
        let level = board
            .trust_level("ai-01")
            .await
            .expect("async operation should succeed");
        assert_eq!(level, "L1.5");

        let item2 = board
            .post(post_req("task 2"))
            .await
            .expect("board post should succeed");
        board
            .claim(item2.id, &RigId::new("ai-01"))
            .await
            .expect("claim should succeed");
        board
            .submit(item2.id, &RigId::new("ai-01"))
            .await
            .expect("submit should succeed");
        board
            .add_stamp(crate::board::AddStampParams {
                target_rig: "ai-01",
                work_item_id: item2.id,
                dimension: "Reliability",
                score: 1.0,
                severity: "Root",
                stamped_by: "reviewer",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .expect("operation should succeed");
        board
            .add_stamp(crate::board::AddStampParams {
                target_rig: "ai-01",
                work_item_id: item2.id,
                dimension: "Helpfulness",
                score: 1.0,
                severity: "Branch",
                stamped_by: "reviewer",
                comment: None,
                active_skill_versions: None,
            })
            .await
            .expect("operation should succeed");
        let level = board
            .trust_level("ai-01")
            .await
            .expect("async operation should succeed");
        assert_eq!(level, "L2");
    }

    #[tokio::test]
    async fn rig_remove_and_get_returns_none() {
        let board = new_board().await;
        board
            .register_rig("temp", "ai", None, None)
            .await
            .expect("register_rig should succeed");
        assert!(
            board
                .get_rig("temp")
                .await
                .expect("async operation should succeed")
                .is_some()
        );
        board
            .remove_rig("temp")
            .await
            .expect("async operation should succeed");
        assert!(
            board
                .get_rig("temp")
                .await
                .expect("async operation should succeed")
                .is_none()
        );
    }

    #[tokio::test]
    async fn system_rigs_created_on_connect() {
        let board = Board::in_memory()
            .await
            .expect("in-memory board should initialize");
        let human = board
            .get_rig("human")
            .await
            .expect("async operation should succeed");
        assert!(human.is_some());
        assert_eq!(human.expect("operation should succeed").rig_type, "system");

        let evolver = board
            .get_rig("evolver")
            .await
            .expect("async operation should succeed");
        assert!(evolver.is_some());
        assert_eq!(
            evolver.expect("operation should succeed").rig_type,
            "system"
        );
    }

    #[tokio::test]
    async fn cannot_remove_system_rig() {
        let board = Board::in_memory()
            .await
            .expect("in-memory board should initialize");
        let result = board.remove_rig("human").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_rig_returns_none_for_unknown() {
        let board = new_board().await;
        let result = board
            .get_rig("nonexistent")
            .await
            .expect("async operation should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn register_and_list_roundtrip() {
        let board = new_board().await;
        board
            .register_rig("r1", "ai", None, None)
            .await
            .expect("register should succeed");
        board
            .register_rig("r2", "ai", Some("researcher"), Some(&["python".into()]))
            .await
            .expect("register should succeed");

        let rigs = board.list_rigs().await.expect("list_rigs should succeed");
        let ids: Vec<&str> = rigs.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"r1"));
        assert!(ids.contains(&"r2"));
    }

    #[tokio::test]
    async fn register_rig_is_idempotent() {
        let board = new_board().await;
        board
            .register_rig("dup", "ai", Some("dev"), None)
            .await
            .expect("first register should succeed");
        board
            .register_rig("dup", "ai", Some("dev"), None)
            .await
            .expect("duplicate register should succeed (idempotent)");

        let rigs = board.list_rigs().await.expect("list_rigs should succeed");
        let count = rigs.iter().filter(|r| r.id == "dup").count();
        assert_eq!(count, 1);
    }
}
