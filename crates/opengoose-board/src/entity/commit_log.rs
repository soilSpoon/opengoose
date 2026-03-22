use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "commit_log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub parent_id: Option<i64>,
    pub root_hash: Vec<u8>,
    pub branch: String,
    pub message: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
