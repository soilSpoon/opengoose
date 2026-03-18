use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "relations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub from_id: i64,
    pub to_id: i64,
    pub relation_type: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
