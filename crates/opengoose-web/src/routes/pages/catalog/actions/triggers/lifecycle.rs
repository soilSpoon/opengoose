use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, TriggerStore};

use crate::data::TriggersPageView;

use super::shared::{danger_notice, selected_page, success_notice};

pub(super) fn toggle_trigger_page(
    db: &Arc<Database>,
    target_name: String,
) -> Result<TriggersPageView> {
    let store = TriggerStore::new(db.clone());

    match store.get_by_name(&target_name)? {
        Some(trigger) => {
            store.set_enabled(&target_name, !trigger.enabled)?;
            let notice = if trigger.enabled {
                success_notice(format!("Trigger `{target_name}` disabled."))
            } else {
                success_notice(format!("Trigger `{target_name}` enabled."))
            };
            selected_page(db, Some(target_name), notice)
        }
        None => selected_page(
            db,
            None,
            danger_notice(format!("Trigger `{target_name}` was not found.")),
        ),
    }
}

pub(super) fn delete_trigger_page(
    db: &Arc<Database>,
    target_name: String,
) -> Result<TriggersPageView> {
    let store = TriggerStore::new(db.clone());
    let removed = store.remove(&target_name)?;
    let notice = if removed {
        success_notice(format!("Trigger `{target_name}` deleted."))
    } else {
        danger_notice(format!("Trigger `{target_name}` was already removed."))
    };
    selected_page(db, None, notice)
}
