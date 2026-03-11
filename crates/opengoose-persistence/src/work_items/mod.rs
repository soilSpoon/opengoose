mod mutations;
mod queries;
#[cfg(test)]
mod tests;
mod types;

use std::sync::Arc;

use crate::db::Database;

pub use types::{WorkItem, WorkStatus};

/// Work item operations on a shared Database.
pub struct WorkItemStore {
    pub(crate) db: Arc<Database>,
}

impl WorkItemStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}
