// Experience Memory — remember/recall/promote API
//
// 에이전트가 작업 중 학습한 지식을 저장하고 이후 작업에서 자동 주입.
// 30일 반감기 가중치로 최근 기억이 우선.

use crate::board::{Board, db_err};
use crate::entity;
use crate::work_item::{BoardError, RigId};
use chrono::{DateTime, Utc};
use sea_orm::prelude::Expr;
use sea_orm::*;

/// A piece of knowledge remembered by a rig.
#[derive(Clone, Debug, PartialEq)]
pub struct Memory {
    pub id: i64,
    pub rig_id: String,
    pub scope: MemoryScope,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

/// Visibility scope for a memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryScope {
    /// Visible only to the originating rig.
    Rig,
    /// Visible to all rigs in the project.
    Project,
    /// Visible globally across all projects.
    Global,
}

impl MemoryScope {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Rig => "rig",
            Self::Project => "project",
            Self::Global => "global",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rig" => Some(Self::Rig),
            "project" => Some(Self::Project),
            "global" => Some(Self::Global),
            _ => None,
        }
    }
}

/// 30-day half-life decay weight. Recent memories score higher.
pub fn memory_weight(last_used_at: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
    let days = (now - last_used_at).num_seconds() as f64 / 86400.0;
    0.5_f64.powf(days / 30.0)
}

impl From<entity::memory::Model> for Memory {
    fn from(m: entity::memory::Model) -> Self {
        Self {
            id: m.id,
            rig_id: m.rig_id,
            scope: MemoryScope::parse(&m.scope).unwrap_or(MemoryScope::Rig),
            content: m.content,
            created_at: m.created_at,
            last_used_at: m.last_used_at,
        }
    }
}

impl Board {
    /// Store a new memory for the given rig (scope = Rig).
    pub async fn remember(&self, rig_id: &RigId, content: &str) -> Result<Memory, BoardError> {
        let now = Utc::now();
        let model = entity::memory::ActiveModel {
            id: NotSet,
            rig_id: Set(rig_id.0.clone()),
            scope: Set(MemoryScope::Rig.as_str().to_string()),
            content: Set(content.to_string()),
            created_at: Set(now),
            last_used_at: Set(now),
        };
        let result = entity::memory::Entity::insert(model)
            .exec(&self.db)
            .await
            .map_err(db_err)?;

        Ok(Memory {
            id: result.last_insert_id,
            rig_id: rig_id.0.clone(),
            scope: MemoryScope::Rig,
            content: content.to_string(),
            created_at: now,
            last_used_at: now,
        })
    }

    /// Recall top-N memories for a rig. Includes rig-scope for this rig
    /// plus project/global scope memories. Sorted by weight descending.
    /// Updates last_used_at for returned memories.
    pub async fn recall(&self, rig_id: &RigId, limit: usize) -> Result<Vec<Memory>, BoardError> {
        let all = entity::memory::Entity::find()
            .all(&self.db)
            .await
            .map_err(db_err)?;

        let now = Utc::now();
        let mut candidates: Vec<(f64, Memory)> = all
            .into_iter()
            .filter(|m| {
                let scope = MemoryScope::parse(&m.scope).unwrap_or(MemoryScope::Rig);
                match scope {
                    MemoryScope::Rig => m.rig_id == rig_id.0,
                    MemoryScope::Project | MemoryScope::Global => true,
                }
            })
            .map(|m| {
                let weight = memory_weight(m.last_used_at, now);
                (weight, Memory::from(m))
            })
            .collect();

        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(limit);

        // Update last_used_at for returned memories
        let ids: Vec<i64> = candidates.iter().map(|(_, m)| m.id).collect();
        if !ids.is_empty() {
            for id in &ids {
                entity::memory::Entity::update_many()
                    .col_expr(entity::memory::Column::LastUsedAt, Expr::value(now))
                    .filter(entity::memory::Column::Id.eq(*id))
                    .exec(&self.db)
                    .await
                    .map_err(db_err)?;
            }
        }

        Ok(candidates.into_iter().map(|(_, m)| m).collect())
    }

    /// Promote a memory to a wider scope.
    pub async fn promote_memory(&self, id: i64, scope: MemoryScope) -> Result<Memory, BoardError> {
        entity::memory::Entity::update_many()
            .col_expr(
                entity::memory::Column::Scope,
                Expr::value(scope.as_str().to_string()),
            )
            .filter(entity::memory::Column::Id.eq(id))
            .exec(&self.db)
            .await
            .map_err(db_err)?;

        let model = entity::memory::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(db_err)?
            .ok_or(BoardError::NotFound(id))?;

        Ok(Memory::from(model))
    }

    /// List memories, optionally filtered by rig.
    pub async fn list_memories(&self, rig_id: Option<&RigId>) -> Result<Vec<Memory>, BoardError> {
        let query = match rig_id {
            Some(rid) => entity::memory::Entity::find()
                .filter(entity::memory::Column::RigId.eq(rid.0.clone())),
            None => entity::memory::Entity::find(),
        };
        let models = query.all(&self.db).await.map_err(db_err)?;
        Ok(models.into_iter().map(Memory::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::new_board;

    #[test]
    fn memory_scope_roundtrip() {
        for scope in [MemoryScope::Rig, MemoryScope::Project, MemoryScope::Global] {
            let s = scope.as_str();
            let parsed = MemoryScope::parse(s).expect("scope should parse");
            assert_eq!(parsed, scope);
        }
        assert!(MemoryScope::parse("unknown").is_none());
    }

    #[test]
    fn memory_weight_decays() {
        let now = Utc::now();
        let recent = memory_weight(now, now);
        assert!((recent - 1.0).abs() < 0.01, "weight at t=0 should be ~1.0");

        let thirty_days_ago = now - chrono::Duration::days(30);
        let half = memory_weight(thirty_days_ago, now);
        assert!(
            (half - 0.5).abs() < 0.01,
            "weight at 30d should be ~0.5, got {half}"
        );

        let sixty_days_ago = now - chrono::Duration::days(60);
        let quarter = memory_weight(sixty_days_ago, now);
        assert!(
            (quarter - 0.25).abs() < 0.01,
            "weight at 60d should be ~0.25, got {quarter}"
        );
    }

    #[tokio::test]
    async fn remember_and_recall() {
        let board = new_board().await;
        let rig = RigId::new("worker-1");

        let mem = board
            .remember(&rig, "JWT uses RS256")
            .await
            .expect("remember should succeed");
        assert_eq!(mem.content, "JWT uses RS256");
        assert_eq!(mem.scope, MemoryScope::Rig);

        let recalled = board.recall(&rig, 10).await.expect("recall should succeed");
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, "JWT uses RS256");
    }

    #[tokio::test]
    async fn recall_includes_project_and_global() {
        let board = new_board().await;
        let rig_a = RigId::new("rig-a");
        let rig_b = RigId::new("rig-b");

        // rig-a stores a memory and promotes it to project scope
        let mem = board
            .remember(&rig_a, "shared knowledge")
            .await
            .expect("remember should succeed");
        board
            .promote_memory(mem.id, MemoryScope::Project)
            .await
            .expect("promote should succeed");

        // rig-b should see it
        let recalled = board
            .recall(&rig_b, 10)
            .await
            .expect("recall should succeed");
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, "shared knowledge");
    }

    #[tokio::test]
    async fn recall_respects_limit() {
        let board = new_board().await;
        let rig = RigId::new("worker-1");

        for i in 0..5 {
            board
                .remember(&rig, &format!("fact {i}"))
                .await
                .expect("remember should succeed");
        }

        let recalled = board.recall(&rig, 3).await.expect("recall should succeed");
        assert_eq!(recalled.len(), 3);
    }

    #[tokio::test]
    async fn promote_memory_changes_scope() {
        let board = new_board().await;
        let rig = RigId::new("worker-1");

        let mem = board
            .remember(&rig, "promote me")
            .await
            .expect("remember should succeed");
        assert_eq!(mem.scope, MemoryScope::Rig);

        let promoted = board
            .promote_memory(mem.id, MemoryScope::Global)
            .await
            .expect("promote should succeed");
        assert_eq!(promoted.scope, MemoryScope::Global);
    }

    #[tokio::test]
    async fn list_memories_filters_by_rig() {
        let board = new_board().await;
        let rig_a = RigId::new("rig-a");
        let rig_b = RigId::new("rig-b");

        board
            .remember(&rig_a, "a's memory")
            .await
            .expect("remember should succeed");
        board
            .remember(&rig_b, "b's memory")
            .await
            .expect("remember should succeed");

        let all = board
            .list_memories(None)
            .await
            .expect("list should succeed");
        assert_eq!(all.len(), 2);

        let only_a = board
            .list_memories(Some(&rig_a))
            .await
            .expect("list should succeed");
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].content, "a's memory");
    }
}
