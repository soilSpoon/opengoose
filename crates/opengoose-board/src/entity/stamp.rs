use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "stamps")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub target_rig: String,
    pub work_item_id: i64,
    pub dimension: String,
    pub score: f32,
    pub severity: String,
    pub stamped_by: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
