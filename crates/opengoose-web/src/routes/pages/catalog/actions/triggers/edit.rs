use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, TriggerStore};

use crate::data::TriggersPageView;
use crate::routes::pages::catalog_forms::TriggerActionForm;

use super::shared::{danger_notice, selected_page, success_notice};

pub(super) fn create_trigger_page(
    db: &Arc<Database>,
    form: TriggerActionForm,
) -> Result<TriggersPageView> {
    let store = TriggerStore::new(db.clone());
    let draft = TriggerDraft::from_create(form);
    if let Some(page) = draft.create_validation_page(db)? {
        return Ok(page);
    }

    match store.create(
        &draft.name,
        &draft.trigger_type,
        &draft.condition_json,
        &draft.team_name,
        &draft.input,
    ) {
        Ok(_) => selected_page(
            db,
            Some(draft.name.clone()),
            success_notice(format!("Trigger `{}` created.", draft.name)),
        ),
        Err(error) => selected_page(db, None, danger_notice(error.to_string())),
    }
}

pub(super) fn update_trigger_page(
    db: &Arc<Database>,
    name: String,
    form: TriggerActionForm,
) -> Result<TriggersPageView> {
    let store = TriggerStore::new(db.clone());
    let draft = TriggerDraft::from_update(name, form);
    if let Some(page) = draft.update_validation_page(db)? {
        return Ok(page);
    }

    let updated = store.update(
        &draft.name,
        &draft.trigger_type,
        &draft.condition_json,
        &draft.team_name,
        &draft.input,
    )?;

    let notice = if updated.is_some() {
        success_notice(format!("Trigger `{}` saved.", draft.name))
    } else {
        danger_notice(format!("Trigger `{}` no longer exists.", draft.name))
    };

    selected_page(db, Some(draft.name), notice)
}

struct TriggerDraft {
    name: String,
    trigger_type: String,
    team_name: String,
    condition_json: String,
    input: String,
}

impl TriggerDraft {
    fn from_create(form: TriggerActionForm) -> Self {
        Self {
            name: form.name.unwrap_or_default().trim().to_string(),
            trigger_type: form.trigger_type.unwrap_or_default().trim().to_string(),
            team_name: form.team_name.unwrap_or_default().trim().to_string(),
            condition_json: form
                .condition_json
                .unwrap_or_else(|| "{}".into())
                .trim()
                .to_string(),
            input: form.input.unwrap_or_default(),
        }
    }

    fn from_update(name: String, form: TriggerActionForm) -> Self {
        Self {
            name,
            trigger_type: form.trigger_type.unwrap_or_default().trim().to_string(),
            team_name: form.team_name.unwrap_or_default().trim().to_string(),
            condition_json: form
                .condition_json
                .unwrap_or_else(|| "{}".into())
                .trim()
                .to_string(),
            input: form.input.unwrap_or_default(),
        }
    }

    fn create_validation_page(&self, db: &Arc<Database>) -> Result<Option<TriggersPageView>> {
        if self.name.is_empty() || self.trigger_type.is_empty() || self.team_name.is_empty() {
            return Ok(Some(selected_page(
                db,
                None,
                danger_notice("Name, type, and team are required to create a trigger.".into()),
            )?));
        }

        self.validate_condition_json()?;
        Ok(None)
    }

    fn update_validation_page(&self, db: &Arc<Database>) -> Result<Option<TriggersPageView>> {
        if self.name.is_empty() || self.trigger_type.is_empty() || self.team_name.is_empty() {
            return Ok(Some(selected_page(
                db,
                Some(self.name.clone()),
                danger_notice("Type and team are required to update a trigger.".into()),
            )?));
        }

        self.validate_condition_json()?;
        Ok(None)
    }

    fn validate_condition_json(&self) -> Result<()> {
        serde_json::from_str::<serde_json::Value>(&self.condition_json)
            .map(|_| ())
            .map_err(|error| error.into())
    }
}
