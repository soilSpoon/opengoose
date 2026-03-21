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
}
