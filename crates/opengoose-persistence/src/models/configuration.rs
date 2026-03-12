use diesel::prelude::*;

use crate::schema::{api_keys, plugins};

#[derive(Queryable, Selectable)]
#[diesel(table_name = plugins)]
pub struct PluginRow {
    pub id: i32,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub capabilities: String,
    pub source_path: String,
    pub enabled: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = plugins)]
pub struct NewPlugin<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub author: Option<&'a str>,
    pub description: Option<&'a str>,
    pub capabilities: &'a str,
    pub source_path: &'a str,
}

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
