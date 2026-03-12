use diesel::prelude::*;

use crate::schema::api_keys;

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = api_keys)]
pub struct ApiKeyRow {
    pub id: String,
    pub key_hash: String,
    pub description: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = api_keys)]
pub struct NewApiKey<'a> {
    pub id: &'a str,
    pub key_hash: &'a str,
    pub description: Option<&'a str>,
}
