use diesel::prelude::*;

use crate::schema::work_item_relations;

#[derive(Queryable, Selectable)]
#[diesel(table_name = work_item_relations)]
pub struct RelationRow {
    pub id: i32,
    pub from_item_id: i32,
    pub to_item_id: i32,
    pub relation_type: String,
    pub created_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = work_item_relations)]
pub struct NewRelation<'a> {
    pub from_item_id: i32,
    pub to_item_id: i32,
    pub relation_type: &'a str,
}
