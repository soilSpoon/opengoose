use std::sync::Arc;

use axum::extract::{Form, State};
use axum::response::Html;
use opengoose_persistence::{Database, TriggerStore};
use opengoose_types::EventBus;
use tracing::error;

use crate::data::{Notice, TriggersPageView, load_triggers_page};
use crate::routes::pages::catalog::pages::{TriggersSpec, render_catalog_spec_page};
use crate::routes::pages::catalog_forms::TriggerActionForm;
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

type TriggerPageResult = Result<TriggersPageView, (axum::http::StatusCode, Html<String>)>;

struct TriggerFields {
    name: String,
    trigger_type: String,
    team_name: String,
    condition_json: String,
    input: String,
}

impl TriggerFields {
    fn new(
        name: String,
        trigger_type: Option<String>,
        team_name: Option<String>,
        condition_json: Option<String>,
        input: Option<String>,
    ) -> Self {
        Self {
            name: name.trim().to_string(),
            trigger_type: trigger_type.unwrap_or_default().trim().to_string(),
            team_name: team_name.unwrap_or_default().trim().to_string(),
            condition_json: condition_json
                .unwrap_or_else(|| "{}".into())
                .trim()
                .to_string(),
            input: input.unwrap_or_default(),
        }
    }

    fn missing_required(&self) -> bool {
        self.name.is_empty() || self.trigger_type.is_empty() || self.team_name.is_empty()
    }

    fn validate_condition_json(&self) -> Result<(), (axum::http::StatusCode, Html<String>)> {
        serde_json::from_str::<serde_json::Value>(&self.condition_json)
            .map(|_| ())
            .map_err(|error| internal_error(error.into()))
    }
}

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
        "create" => create_trigger_page(
            &store,
            db,
            form.name,
            form.trigger_type,
            form.team_name,
            form.condition_json,
            form.input,
        )?,
        "update" => update_trigger_page(
            &store,
            db,
            target_name,
            form.trigger_type,
            form.team_name,
            form.condition_json,
            form.input,
        )?,
        "toggle" => toggle_trigger_page(&store, db, target_name)?,
        "delete" => delete_trigger_page(&store, db, target_name)?,
        "test" => test_trigger_page(&store, db, event_bus, target_name)?,
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
    db: Arc<Database>,
    name: Option<String>,
    trigger_type: Option<String>,
    team_name: Option<String>,
    condition_json: Option<String>,
    input: Option<String>,
) -> TriggerPageResult {
    let fields = TriggerFields::new(
        name.unwrap_or_default(),
        trigger_type,
        team_name,
        condition_json,
        input,
    );

    if fields.missing_required() {
        return load_trigger_page_with_notice(
            db,
            None,
            danger_notice("Name, type, and team are required to create a trigger."),
        );
    }

    fields.validate_condition_json()?;
    match store.create(
        &fields.name,
        &fields.trigger_type,
        &fields.condition_json,
        &fields.team_name,
        &fields.input,
    ) {
        Ok(_) => load_trigger_page_with_notice(
            db,
            Some(fields.name.clone()),
            success_notice(format!("Trigger `{}` created.", fields.name)),
        ),
        Err(error) => load_trigger_page_with_notice(db, None, danger_notice(error.to_string())),
    }
}

fn update_trigger_page(
    store: &TriggerStore,
    db: Arc<Database>,
    name: String,
    trigger_type: Option<String>,
    team_name: Option<String>,
    condition_json: Option<String>,
    input: Option<String>,
) -> TriggerPageResult {
    let fields = TriggerFields::new(name, trigger_type, team_name, condition_json, input);

    if fields.missing_required() {
        return load_trigger_page_with_notice(
            db,
            Some(fields.name.clone()),
            danger_notice("Type and team are required to update a trigger."),
        );
    }

    fields.validate_condition_json()?;
    let updated = store
        .update(
            &fields.name,
            &fields.trigger_type,
            &fields.condition_json,
            &fields.team_name,
            &fields.input,
        )
        .map_err(|error| internal_error(error.into()))?;

    let notice = if updated.is_some() {
        success_notice(format!("Trigger `{}` saved.", fields.name))
    } else {
        danger_notice(format!("Trigger `{}` no longer exists.", fields.name))
    };

    load_trigger_page_with_notice(db, Some(fields.name), notice)
}

fn toggle_trigger_page(
    store: &TriggerStore,
    db: Arc<Database>,
    target_name: String,
) -> TriggerPageResult {
    match store
        .get_by_name(&target_name)
        .map_err(|error| internal_error(error.into()))?
    {
        Some(trigger) => {
            store
                .set_enabled(&target_name, !trigger.enabled)
                .map_err(|error| internal_error(error.into()))?;
            let notice = if trigger.enabled {
                success_notice(format!("Trigger `{target_name}` disabled."))
            } else {
                success_notice(format!("Trigger `{target_name}` enabled."))
            };
            load_trigger_page_with_notice(db, Some(target_name), notice)
        }
        None => load_trigger_page_with_notice(
            db,
            None,
            danger_notice(format!("Trigger `{target_name}` was not found.")),
        ),
    }
}

fn delete_trigger_page(
    store: &TriggerStore,
    db: Arc<Database>,
    target_name: String,
) -> TriggerPageResult {
    let removed = store
        .remove(&target_name)
        .map_err(|error| internal_error(error.into()))?;
    let notice = if removed {
        success_notice(format!("Trigger `{target_name}` deleted."))
    } else {
        danger_notice(format!("Trigger `{target_name}` was already removed."))
    };

    load_trigger_page_with_notice(db, None, notice)
}

fn test_trigger_page(
    store: &TriggerStore,
    db: Arc<Database>,
    event_bus: EventBus,
    target_name: String,
) -> TriggerPageResult {
    match store
        .get_by_name(&target_name)
        .map_err(|error| internal_error(error.into()))?
    {
        Some(trigger) => {
            let trigger_name = trigger.name.clone();
            let team_name = trigger.team_name.clone();
            let input = if trigger.input.trim().is_empty() {
                format!(
                    "Test run fired from the web dashboard for trigger {}",
                    trigger.name
                )
            } else {
                trigger.input.clone()
            };

            queue_trigger_test_run(db.clone(), event_bus, trigger_name, team_name, input);
            load_trigger_page_with_notice(
                db,
                Some(target_name.clone()),
                success_notice(format!(
                    "Trigger `{target_name}` test queued. Check Runs for progress."
                )),
            )
        }
        None => load_trigger_page_with_notice(
            db,
            None,
            danger_notice(format!("Trigger `{target_name}` was not found.")),
        ),
    }
}

fn queue_trigger_test_run(
    db: Arc<Database>,
    event_bus: EventBus,
    trigger_name: String,
    team_name: String,
    input: String,
) {
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

fn load_trigger_page_with_notice(
    db: Arc<Database>,
    selected: Option<String>,
    notice: Notice,
) -> TriggerPageResult {
    let mut page = load_triggers_page(db, selected).map_err(internal_error)?;
    page.selected.notice = Some(notice);
    Ok(page)
}

fn success_notice(text: impl Into<String>) -> Notice {
    Notice {
        text: text.into(),
        tone: "success",
    }
}

fn danger_notice(text: impl Into<String>) -> Notice {
    Notice {
        text: text.into(),
        tone: "danger",
    }
}
