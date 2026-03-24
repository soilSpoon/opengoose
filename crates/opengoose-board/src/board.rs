// Wanted Board — SQLite 기반 pull 작업 분배
//
// SeaORM + SQLite. 모든 메서드가 async.
// 상태 변경 메서드는 트랜잭션으로 원자성 보장.

use crate::branch::Branch;
use crate::entity;
use crate::merge::MergeResult;
use crate::stamps::Severity;
use crate::store::CowStore;
use crate::work_item::{BoardError, RigId};
use chrono::{DateTime, Utc};
use sea_orm::*;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

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
pub(crate) fn stamp_weighted_value(stamp: &entity::stamp::Model, now: DateTime<Utc>) -> f32 {
    let days = (now - stamp.timestamp).num_seconds() as f32 / 86400.0;
    let decay = 0.5_f32.powf(days / 30.0);
    let weight = Severity::parse(&stamp.severity)
        .unwrap_or(Severity::Leaf)
        .weight();
    weight * stamp.score * decay
}

pub struct Board {
    pub(crate) db: DatabaseConnection,
    pub(crate) notify: Arc<Notify>,
    pub(crate) stamp_notify: Arc<Notify>,
    pub(crate) store: Mutex<CowStore>,
}

impl Board {
    pub async fn connect(db_url: &str) -> Result<Self, BoardError> {
        let db = Database::connect(db_url).await.map_err(db_err)?;
        Self::create_tables(&db).await?;
        Self::ensure_columns(&db).await?;
        Self::ensure_system_rigs(&db).await?;
        let store = CowStore::restore(&db).await?;
        Ok(Self {
            db,
            notify: Arc::new(Notify::new()),
            stamp_notify: Arc::new(Notify::new()),
            store: Mutex::new(store),
        })
    }

    pub async fn in_memory() -> Result<Self, BoardError> {
        Self::connect("sqlite::memory:").await
    }

    pub(crate) async fn create_tables(db: &DatabaseConnection) -> Result<(), BoardError> {
        let backend = db.get_database_backend();
        let schema = Schema::new(backend);

        for mut stmt in [
            schema.create_table_from_entity(entity::work_item::Entity),
            schema.create_table_from_entity(entity::relation::Entity),
            schema.create_table_from_entity(entity::stamp::Entity),
            schema.create_table_from_entity(entity::rig::Entity),
            schema.create_table_from_entity(entity::commit_log::Entity),
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

    // ── CoW Store: branch/merge ──────────────────────────────

    pub async fn branch(&self, rig_id: &RigId) -> Branch {
        self.store.lock().await.branch(rig_id)
    }

    pub async fn merge(&self, branch: Branch) -> Result<MergeResult, BoardError> {
        let mut store = self.store.lock().await;
        // Stage merge on a clone — only swap in after persist succeeds
        let mut staged = store.clone();
        let result = staged.merge(branch)?;
        staged.persist(&self.db).await?;
        *store = staged;
        Ok(result)
    }

    pub async fn discard_branch(&self, branch: Branch) {
        self.store.lock().await.discard(branch);
    }

    /// Get blocked item IDs (public for cross-crate access by Worker).
    pub async fn get_blocked_ids(&self) -> Result<std::collections::HashSet<i64>, BoardError> {
        self.blocked_item_ids().await
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

    /// 순환 감지. 전체 relation 테이블을 한 번 로드하여 in-memory BFS.
    async fn would_create_cycle(&self, from: i64, to: i64) -> Result<bool, BoardError> {
        let all_relations = entity::relation::Entity::find()
            .all(&self.db)
            .await
            .map_err(db_err)?;

        // RelationGraph에 로드하여 순환 감지를 위임
        let mut graph = crate::relations::RelationGraph::new();
        for rel in &all_relations {
            graph.restore_edge(
                rel.from_id,
                rel.to_id,
                crate::relations::RelationType::Blocks,
            );
        }
        Ok(graph
            .add(from, to, crate::relations::RelationType::Blocks)
            .is_err())
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

pub(crate) fn db_err(e: DbErr) -> BoardError {
    BoardError::DbError(e.to_string())
}

// ── 테스트 — board.rs 고유 테스트만 (CRUD/stamp/rig 테스트는 각 모듈 참조) ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::post_req;
    use crate::work_item::{RigId, Status};

    #[tokio::test]
    async fn cycle_detection() {
        let board = Board::in_memory().await.expect("in-memory board should initialize");
        for title in ["a", "b", "c"] {
            board.post(post_req(title)).await.expect("board post should succeed");
        }
        board.add_dependency(1, 2).await.expect("async operation should succeed");
        board.add_dependency(2, 3).await.expect("async operation should succeed");
        assert!(board.add_dependency(3, 1).await.is_err());
    }

    #[tokio::test]
    async fn self_cycle_rejected() {
        let board = Board::in_memory().await.expect("in-memory board should initialize");
        board.post(post_req("a")).await.expect("board post should succeed");
        assert!(board.add_dependency(1, 1).await.is_err());
    }

    #[test]
    fn db_err_wraps_db_error_into_board_error() {
        use sea_orm::DbErr;
        let board_err = db_err(DbErr::Custom("test db error".into()));
        assert!(matches!(board_err, BoardError::DbError(_)));
        if let BoardError::DbError(msg) = board_err {
            assert!(msg.contains("test db error"));
        }
    }

    #[tokio::test]
    async fn connect_twice_covers_ensure_system_rigs_existing_branch() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let db_path = tmp.path().join("test.db");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());

        let _board1 = Board::connect(&url).await.expect("async operation should succeed");
        let board2 = Board::connect(&url).await.expect("async operation should succeed");
        let rigs = board2.list_rigs().await.expect("list_rigs should succeed");
        assert!(rigs.iter().any(|r| r.id == "human"));
        assert!(rigs.iter().any(|r| r.id == "evolver"));
    }

    // ── CoW Store integration tests ─────────────────────

    #[tokio::test]
    async fn board_branch_and_merge_lifecycle() {
        let board = Board::in_memory().await.expect("in-memory board should initialize");
        let rig_id = RigId::new("worker-1");
        board
            .register_rig("worker-1", "ai", None, None)
            .await
            .expect("operation should succeed");

        let item = board.post(post_req("Test task")).await.expect("board post should succeed");

        let mut branch = board.branch(&rig_id).await;
        assert_eq!(branch.list().count(), 1);

        branch.update(item.id, |i| {
            i.status = Status::Claimed;
            i.claimed_by = Some(rig_id.clone());
            i.updated_at = Utc::now();
        });

        let result = board.merge(branch).await.expect("async operation should succeed");
        assert_eq!(result.merged_items.len(), 1);

        let updated = board.get(item.id).await.expect("get should succeed").expect("operation should succeed");
        assert_eq!(updated.status, Status::Claimed);
    }

    #[tokio::test]
    async fn board_discard_branch_leaves_main_unchanged() {
        let board = Board::in_memory().await.expect("in-memory board should initialize");
        let item = board.post(post_req("Test")).await.expect("board post should succeed");

        let mut branch = board.branch(&RigId::new("worker")).await;
        branch.update(item.id, |i| i.status = Status::Claimed);
        board.discard_branch(branch).await;

        let unchanged = board.get(item.id).await.expect("get should succeed").expect("operation should succeed");
        assert_eq!(unchanged.status, Status::Open);
    }

    #[tokio::test]
    async fn board_post_syncs_to_cowstore() {
        let board = Board::in_memory().await.expect("in-memory board should initialize");
        board.post(post_req("Item 1")).await.expect("board post should succeed");
        board.post(post_req("Item 2")).await.expect("board post should succeed");

        let branch = board.branch(&RigId::new("test")).await;
        assert_eq!(branch.list().count(), 2);
    }
}
