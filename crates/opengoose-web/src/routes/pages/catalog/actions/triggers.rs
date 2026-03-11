use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Form, State};
use opengoose_persistence::{Database, TriggerStore};
use opengoose_types::EventBus;
use tracing::error;

use crate::data::{Notice, TriggersPageView, load_triggers_page};
use crate::routes::pages::catalog::pages::{TriggersSpec, render_catalog_spec_page};
use crate::routes::pages::catalog_forms::TriggerActionForm;
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

pub(crate) async fn trigger_action(
    State(state): State<PageState>,
    Form(form): Form<TriggerActionForm>,
) -> WebResult {
    let db = state.db.clone();
    let event_bus = state.event_bus.clone();
    let store = TriggerStore::new(db.clone());
    let target_name = form
        .original_name
        .clone()
        .or_else(|| form.name.clone())
        .unwrap_or_default();

    let page = match form.intent.as_str() {
        "create" => create_trigger_page(&store, &db, form).map_err(internal_error)?,
        "update" => update_trigger_page(&store, &db, target_name, form).map_err(internal_error)?,
        "toggle" => toggle_trigger_page(&store, &db, target_name).map_err(internal_error)?,
        "delete" => delete_trigger_page(&store, &db, target_name).map_err(internal_error)?,
        "test" => test_trigger_page(&store, &db, event_bus, target_name).map_err(internal_error)?,
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::response::Html("Unsupported trigger action.".into()),
            ));
        }
    };

    render_catalog_spec_page::<TriggersSpec>(page)
}

fn create_trigger_page(
    store: &TriggerStore,
    db: &Arc<Database>,
    form: TriggerActionForm,
) -> Result<TriggersPageView> {
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

fn update_trigger_page(
    store: &TriggerStore,
    db: &Arc<Database>,
    name: String,
    form: TriggerActionForm,
) -> Result<TriggersPageView> {
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

fn toggle_trigger_page(
    store: &TriggerStore,
    db: &Arc<Database>,
    target_name: String,
) -> Result<TriggersPageView> {
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

fn delete_trigger_page(
    store: &TriggerStore,
    db: &Arc<Database>,
    target_name: String,
) -> Result<TriggersPageView> {
    let removed = store.remove(&target_name)?;
    let notice = if removed {
        success_notice(format!("Trigger `{target_name}` deleted."))
    } else {
        danger_notice(format!("Trigger `{target_name}` was already removed."))
    };
    selected_page(db, None, notice)
}

fn test_trigger_page(
    store: &TriggerStore,
    db: &Arc<Database>,
    event_bus: EventBus,
    target_name: String,
) -> Result<TriggersPageView> {
    match store.get_by_name(&target_name)? {
        Some(trigger) => {
            spawn_trigger_test(
                db.clone(),
                event_bus,
                &trigger.name,
                &trigger.team_name,
                &trigger.input,
            );
            selected_page(
                db,
                Some(target_name.clone()),
                success_notice(format!(
                    "Trigger `{target_name}` test queued. Check Runs for progress."
                )),
            )
        }
        None => selected_page(
            db,
            None,
            danger_notice(format!("Trigger `{target_name}` was not found.")),
        ),
    }
}

fn spawn_trigger_test(
    db: Arc<Database>,
    event_bus: EventBus,
    trigger_name: &str,
    team_name: &str,
    input: &str,
) {
    let trigger_name = trigger_name.to_string();
    let team_name = team_name.to_string();
    let input = if input.trim().is_empty() {
        format!("Test run fired from the web dashboard for trigger {trigger_name}")
    } else {
        input.to_string()
    };

    tokio::spawn(async move {
        match opengoose_teams::run_headless(&team_name, &input, db.clone(), event_bus).await {
            Ok(_) => {
                let store = TriggerStore::new(db);
                if let Err(error) = store.mark_fired(&trigger_name) {
                    error!(trigger = %trigger_name, %error, "failed to mark trigger fired after page test");
                }
            }
            Err(error) => {
                error!(trigger = %trigger_name, team = %team_name, %error, "page trigger test failed");
            }
        }
    });
}

fn selected_page(
    db: &Arc<Database>,
    selected: Option<String>,
    notice: Notice,
) -> Result<TriggersPageView> {
    let mut page = load_triggers_page(db.clone(), selected)?;
    page.selected.notice = Some(notice);
    Ok(page)
}

fn success_notice(text: String) -> Notice {
    Notice {
        text,
        tone: "success",
    }
}

fn danger_notice(text: String) -> Notice {
    Notice {
        text,
        tone: "danger",
    }
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
