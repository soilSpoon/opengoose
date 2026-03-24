use std::collections::BTreeMap;

use sea_orm::{
    ActiveModelTrait, DatabaseConnection, EntityTrait, QueryOrder, Set, TransactionTrait,
};
use sha2::{Digest, Sha256};

use crate::board::db_err;
use crate::work_item::{BoardError, RigId, WorkItem};

use super::{Commit, CommitId, CowStore};

impl CowStore {
    /// Persist current main state and commit log to SQLite.
    pub async fn persist(&self, db: &DatabaseConnection) -> Result<(), BoardError> {
        use crate::entity;

        let txn = db.begin().await.map_err(db_err)?;

        for item in self.main.values() {
            let tags_json = serde_json::to_string(&item.tags).unwrap_or_default();
            let active = entity::work_item::ActiveModel {
                id: Set(item.id),
                title: Set(item.title.clone()),
                description: Set(item.description.clone()),
                status: Set(item.status),
                priority: Set(item.priority),
                tags: Set(Some(tags_json)),
                created_by: Set(item.created_by.0.clone()),
                claimed_by: Set(item.claimed_by.as_ref().map(|r| r.0.clone())),
                created_at: Set(item.created_at),
                updated_at: Set(item.updated_at),
            };
            entity::work_item::Entity::insert(active)
                .on_conflict(
                    sea_orm::sea_query::OnConflict::column(entity::work_item::Column::Id)
                        .update_columns([
                            entity::work_item::Column::Title,
                            entity::work_item::Column::Description,
                            entity::work_item::Column::Status,
                            entity::work_item::Column::Priority,
                            entity::work_item::Column::Tags,
                            entity::work_item::Column::ClaimedBy,
                            entity::work_item::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await
                .map_err(db_err)?;
        }

        for commit in &self.commits {
            let exists: Vec<entity::commit_log::Model> =
                entity::commit_log::Entity::find_by_id(commit.id.0 as i64)
                    .all(&txn)
                    .await
                    .map_err(db_err)?;
            if exists.is_empty() {
                let active = entity::commit_log::ActiveModel {
                    id: Set(commit.id.0 as i64),
                    parent_id: Set(commit.parent.map(|p| p.0 as i64)),
                    root_hash: Set(commit.root_hash.to_vec()),
                    branch: Set(commit.branch.0.clone()),
                    message: Set(commit.message.clone()),
                    created_at: Set(commit.timestamp),
                };
                active.insert(&txn).await.map_err(db_err)?;
            }
        }

        txn.commit().await.map_err(db_err)?;
        Ok(())
    }

    /// Restore CowStore state from SQLite.
    pub async fn restore(db: &DatabaseConnection) -> Result<Self, BoardError> {
        use crate::entity;

        let models = entity::work_item::Entity::find()
            .all(db)
            .await
            .map_err(db_err)?;

        let mut items = BTreeMap::new();
        for model in models {
            let item = WorkItem::from(model);
            items.insert(item.id, item);
        }

        let commit_models = entity::commit_log::Entity::find()
            .order_by_asc(entity::commit_log::Column::Id)
            .all(db)
            .await
            .map_err(db_err)?;

        let commits: Vec<Commit> = commit_models
            .into_iter()
            .map(|m| Commit {
                id: CommitId(m.id as u64),
                parent: m.parent_id.map(|p| CommitId(p as u64)),
                root_hash: {
                    let mut hash = [0u8; 32];
                    let len = m.root_hash.len().min(32);
                    hash[..len].copy_from_slice(&m.root_hash[..len]);
                    hash
                },
                branch: RigId::new(m.branch),
                message: m.message,
                timestamp: m.created_at,
            })
            .collect();

        Ok(Self::from_items(items, commits))
    }

    pub(super) fn compute_root_hash(data: &BTreeMap<i64, WorkItem>) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for (id, item) in data.iter() {
            hasher.update(id.to_le_bytes());
            hasher.update(serde_json::to_vec(item).unwrap_or_default());
        }
        hasher.finalize().into()
    }

    pub(super) fn append_commit(&mut self, branch: &RigId, message: String) -> Commit {
        let root_hash = Self::compute_root_hash(&self.main);
        let parent = self.commits.last().map(|c| c.id);
        let id = CommitId(self.next_commit_id);
        self.next_commit_id += 1;
        let commit = Commit {
            id,
            parent,
            root_hash,
            branch: branch.clone(),
            message,
            timestamp: chrono::Utc::now(),
        };
        self.commits.push(commit.clone());
        commit
    }
}
