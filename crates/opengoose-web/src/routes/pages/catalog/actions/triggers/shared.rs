use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;

use crate::data::{Notice, TriggersPageView, load_triggers_page};

pub(super) fn selected_page(
    db: &Arc<Database>,
    selected: Option<String>,
    notice: Notice,
) -> Result<TriggersPageView> {
    let mut page = load_triggers_page(db.clone(), selected)?;
    page.selected.notice = Some(notice);
    Ok(page)
}

pub(super) fn success_notice(text: String) -> Notice {
    Notice {
        text,
        tone: "success",
    }
}

pub(super) fn danger_notice(text: String) -> Notice {
    Notice {
        text,
        tone: "danger",
    }
}
