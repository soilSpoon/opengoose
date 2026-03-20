// Wanted Board — SQLite 기반 pull 작업 분배
//
// SeaORM + SQLite. 모든 메서드가 async.
// 상태 변경 메서드는 트랜잭션으로 원자성 보장.

use crate::entity;
use crate::stamps::Severity;
use crate::work_item::BoardError;
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

pub(crate) fn db_err(e: DbErr) -> BoardError {
    BoardError::DbError(e.to_string())
}

// ── 테스트 ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_item::{PostWorkItem, Priority, RigId, Status};

    fn post_req(title: &str) -> PostWorkItem {
        PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
        }
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

    async fn new_board() -> Board {
        Board::in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn cycle_detection() {
        let board = Board::in_memory().await.unwrap();
        board.post(crate::work_item::PostWorkItem {
            title: "a".into(),
            description: String::new(),
            created_by: crate::work_item::RigId::new("user"),
            priority: crate::work_item::Priority::P1,
            tags: vec![],
        }).await.unwrap();
        board.post(crate::work_item::PostWorkItem {
            title: "b".into(),
            description: String::new(),
            created_by: crate::work_item::RigId::new("user"),
            priority: crate::work_item::Priority::P1,
            tags: vec![],
        }).await.unwrap();
        board.post(crate::work_item::PostWorkItem {
            title: "c".into(),
            description: String::new(),
            created_by: crate::work_item::RigId::new("user"),
            priority: crate::work_item::Priority::P1,
            tags: vec![],
        }).await.unwrap();

        board.add_dependency(1, 2).await.unwrap();
        board.add_dependency(2, 3).await.unwrap();
        assert!(board.add_dependency(3, 1).await.is_err());
    }

    #[tokio::test]
    async fn self_cycle_rejected() {
        let board = Board::in_memory().await.unwrap();
        board.post(crate::work_item::PostWorkItem {
            title: "a".into(),
            description: String::new(),
            created_by: crate::work_item::RigId::new("user"),
            priority: crate::work_item::Priority::P1,
            tags: vec![],
        }).await.unwrap();
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
    async fn list_rigs_returns_registered_rigs() {
        let board = new_board().await;
        board.register_rig("worker-1", "ai", Some("developer"), None).await.unwrap();
        board.register_rig("worker-2", "ai", None, None).await.unwrap();

        let rigs = board.list_rigs().await.unwrap();
        // system rigs (human, evolver) + 2 registered
        assert!(rigs.iter().any(|r| r.id == "worker-1"));
        assert!(rigs.iter().any(|r| r.id == "worker-2"));
    }

    #[tokio::test]
    async fn stamps_for_item_returns_stamps() {
        let board = Board::in_memory().await.unwrap();
        let item = board.post(post_req("task")).await.unwrap();

        board.add_stamp(AddStampParams {
            target_rig: "rig-a",
            work_item_id: item.id,
            dimension: "Quality",
            score: 0.7,
            severity: "Leaf",
            stamped_by: "human",
            comment: None,
            active_skill_versions: None,
        }).await.unwrap();
        board.add_stamp(AddStampParams {
            target_rig: "rig-a",
            work_item_id: item.id,
            dimension: "Reliability",
            score: 0.5,
            severity: "Leaf",
            stamped_by: "human",
            comment: None,
            active_skill_versions: None,
        }).await.unwrap();

        let stamps = board.stamps_for_item(item.id).await.unwrap();
        assert_eq!(stamps.len(), 2);
        assert!(stamps.iter().any(|s| s.dimension == "Quality"));
        assert!(stamps.iter().any(|s| s.dimension == "Reliability"));
    }

    #[tokio::test]
    async fn recent_low_stamps_returns_only_low_and_recent() {
        let board = Board::in_memory().await.unwrap();
        let item = board.post(post_req("task")).await.unwrap();

        // Low score stamp
        let id1 = board.add_stamp(AddStampParams {
            target_rig: "rig-a",
            work_item_id: item.id,
            dimension: "Quality",
            score: 0.1,
            severity: "Leaf",
            stamped_by: "human",
            comment: None,
            active_skill_versions: None,
        }).await.unwrap();
        // High score stamp - should not appear
        board.add_stamp(AddStampParams {
            target_rig: "rig-a",
            work_item_id: item.id,
            dimension: "Reliability",
            score: 0.9,
            severity: "Leaf",
            stamped_by: "human",
            comment: None,
            active_skill_versions: None,
        }).await.unwrap();

        let recent = board.recent_low_stamps(0.3, 30).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, id1);
    }

    #[tokio::test]
    async fn recent_low_stamps_empty_when_threshold_zero() {
        let board = Board::in_memory().await.unwrap();
        let item = board.post(post_req("task")).await.unwrap();
        board.add_stamp(AddStampParams {
            target_rig: "rig-a",
            work_item_id: item.id,
            dimension: "Quality",
            score: 0.1,
            severity: "Leaf",
            stamped_by: "human",
            comment: None,
            active_skill_versions: None,
        }).await.unwrap();

        // threshold 0 means nothing is "low"
        let recent = board.recent_low_stamps(0.0, 30).await.unwrap();
        assert!(recent.is_empty());
    }

    #[tokio::test]
    async fn mark_stamp_evolved_is_idempotent() {
        let board = Board::in_memory().await.unwrap();
        let item = board.post(post_req("task")).await.unwrap();

        let id = board.add_stamp(AddStampParams {
            target_rig: "rig-a",
            work_item_id: item.id,
            dimension: "Quality",
            score: 0.2,
            severity: "Leaf",
            stamped_by: "human",
            comment: None,
            active_skill_versions: None,
        }).await.unwrap();

        let first = board.mark_stamp_evolved(id).await.unwrap();
        assert!(first, "first call should return true");

        let second = board.mark_stamp_evolved(id).await.unwrap();
        assert!(!second, "second call should return false (already evolved)");
    }

    #[tokio::test]
    async fn mark_stuck_wrong_rig_fails() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        board.claim(item.id, &RigId::new("worker-a")).await.unwrap();

        let err = board.mark_stuck(item.id, &RigId::new("worker-b")).await;
        assert!(matches!(err, Err(BoardError::NotClaimedBy { .. })));
    }

    #[tokio::test]
    async fn unclaim_wrong_rig_fails() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        board.claim(item.id, &RigId::new("worker-a")).await.unwrap();

        let err = board.unclaim(item.id, &RigId::new("worker-b")).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn weighted_score_returns_positive_for_good_stamps() {
        let board = Board::in_memory().await.unwrap();
        let item = board.post(post_req("task")).await.unwrap();
        board.add_stamp(AddStampParams {
            target_rig: "rig-x",
            work_item_id: item.id,
            dimension: "Quality",
            score: 1.0,
            severity: "Root",
            stamped_by: "human",
            comment: None,
            active_skill_versions: None,
        }).await.unwrap();

        let score = board.weighted_score("rig-x").await.unwrap();
        assert!(score > 0.0);
    }

    #[tokio::test]
    async fn weighted_score_zero_for_no_stamps() {
        let board = Board::in_memory().await.unwrap();
        let score = board.weighted_score("no-stamps-rig").await.unwrap();
        assert_eq!(score, 0.0);
    }

    #[tokio::test]
    async fn register_rig_is_idempotent() {
        let board = new_board().await;
        board.register_rig("worker-dup", "ai", None, None).await.unwrap();
        // Second call should succeed silently (early return if already exists)
        board.register_rig("worker-dup", "ai", Some("developer"), None).await.unwrap();

        let rig = board.get_rig("worker-dup").await.unwrap().unwrap();
        // Still the original (first registration wins)
        assert!(rig.recipe.is_none());
    }

    #[tokio::test]
    async fn remove_nonexistent_rig_is_noop() {
        let board = new_board().await;
        // Removing a rig that doesn't exist should succeed (no-op)
        board.remove_rig("ghost-rig").await.unwrap();
    }

    #[tokio::test]
    async fn abandon_claimed_item_fails() {
        let board = new_board().await;
        let item = board.post(post_req("task")).await.unwrap();
        board.claim(item.id, &RigId::new("worker")).await.unwrap();

        // Claimed → Abandoned is not a valid transition
        let err = board.abandon(item.id).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn wait_for_claimable_unblocks_when_item_posted() {
        use std::sync::Arc;
        let board = Arc::new(new_board().await);
        let board2 = board.clone();

        // Start a task waiting for a claimable item
        let wait_handle = tokio::spawn(async move {
            board2.wait_for_claimable().await;
        });

        // Post an item — this calls notify_waiters(), unblocking the waiter
        board.post(post_req("trigger")).await.unwrap();

        // Should complete within a reasonable timeout
        tokio::time::timeout(std::time::Duration::from_millis(500), wait_handle)
            .await
            .expect("wait_for_claimable timed out")
            .expect("wait task panicked");
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
        // Connecting to the same SQLite file twice triggers ensure_system_rigs
        // on an already-initialized DB — the second call finds the rigs exist
        // (existing.is_none() == false), covering board.rs line 601.
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());

        // First connection: creates tables and inserts system rigs
        let _board1 = Board::connect(&url).await.unwrap();

        // Second connection: rigs already exist → if existing.is_none() = false
        let board2 = Board::connect(&url).await.unwrap();
        let rigs = board2.list_rigs().await.unwrap();
        assert!(rigs.iter().any(|r| r.id == "human"));
        assert!(rigs.iter().any(|r| r.id == "evolver"));
    }
}
