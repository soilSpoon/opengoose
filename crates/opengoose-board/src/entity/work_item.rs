use crate::work_item::{Priority, RigId, Status, WorkItem};
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "work_items")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub title: String,
    pub description: String,
    pub status: Status,
    pub priority: Priority,
    pub created_by: String,
    pub claimed_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for WorkItem {
    fn from(m: Model) -> Self {
        WorkItem {
            id: m.id,
            title: m.title,
            description: m.description,
            status: m.status,
            priority: m.priority,
            created_by: RigId::new(m.created_by),
            claimed_by: m.claimed_by.map(RigId::new),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
