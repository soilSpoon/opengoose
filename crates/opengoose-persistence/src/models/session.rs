use diesel::prelude::*;

use crate::schema::sessions;

#[derive(Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSession<'a> {
    pub session_key: &'a str,
    pub selected_model: Option<&'a str>,
}
