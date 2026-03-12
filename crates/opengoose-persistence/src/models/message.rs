use diesel::prelude::*;

use crate::schema::messages;

#[derive(Insertable)]
#[diesel(table_name = messages)]
pub struct NewMessage<'a> {
    pub session_key: &'a str,
    pub role: &'a str,
    pub content: &'a str,
    pub author: Option<&'a str>,
}
