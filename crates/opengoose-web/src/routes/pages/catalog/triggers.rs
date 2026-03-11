use axum::extract::{Form, Query, State};
use opengoose_persistence::TriggerStore;
use serde::Deserialize;
use tracing::error;

use crate::data::{Notice, load_triggers_page};
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

use super::render::render_triggers_page;

#[derive(Deserialize, Default)]
pub(crate) struct TriggerQuery {
    pub(crate) trigger: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TriggerActionForm {
    pub(crate) intent: String,
    pub(crate) original_name: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) trigger_type: Option<String>,
    pub(crate) team_name: Option<String>,
    pub(crate) condition_json: Option<String>,
    pub(crate) input: Option<String>,
}

pub(crate) async fn triggers(
    State(state): State<PageState>,
    Query(query): Query<TriggerQuery>,
) -> WebResult {
    let page = load_triggers_page(state.db, query.trigger).map_err(internal_error)?;
    render_triggers_page(page)
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
        "create" => {
            let name = form.name.unwrap_or_default().trim().to_string();
            let trigger_type = form.trigger_type.unwrap_or_default().trim().to_string();
            let team_name = form.team_name.unwrap_or_default().trim().to_string();
            let condition_json = form
                .condition_json
                .unwrap_or_else(|| "{}".into())
                .trim()
                .to_string();
            let input = form.input.unwrap_or_default();

            if name.is_empty() || trigger_type.is_empty() || team_name.is_empty() {
                let mut page = load_triggers_page(db.clone(), None).map_err(internal_error)?;
                page.selected.notice = Some(Notice {
                    text: "Name, type, and team are required to create a trigger.".into(),
                    tone: "danger",
                });
                page
            } else {
                serde_json::from_str::<serde_json::Value>(&condition_json)
                    .map_err(|error| internal_error(error.into()))?;
                match store.create(&name, &trigger_type, &condition_json, &team_name, &input) {
                    Ok(_) => {
                        let mut page = load_triggers_page(db.clone(), Some(name.clone()))
                            .map_err(internal_error)?;
                        page.selected.notice = Some(Notice {
                            text: format!("Trigger `{name}` created."),
                            tone: "success",
                        });
                        page
                    }
                    Err(error) => {
                        let mut page =
                            load_triggers_page(db.clone(), None).map_err(internal_error)?;
                        page.selected.notice = Some(Notice {
                            text: error.to_string(),
                            tone: "danger",
                        });
                        page
                    }
                }
            }
        }
        "update" => {
            let name = target_name.clone();
            let trigger_type = form.trigger_type.unwrap_or_default().trim().to_string();
            let team_name = form.team_name.unwrap_or_default().trim().to_string();
            let condition_json = form
                .condition_json
                .unwrap_or_else(|| "{}".into())
                .trim()
                .to_string();
            let input = form.input.unwrap_or_default();

            if name.is_empty() || trigger_type.is_empty() || team_name.is_empty() {
                let mut page =
                    load_triggers_page(db.clone(), Some(name)).map_err(internal_error)?;
                page.selected.notice = Some(Notice {
                    text: "Type and team are required to update a trigger.".into(),
                    tone: "danger",
                });
                page
            } else {
                serde_json::from_str::<serde_json::Value>(&condition_json)
                    .map_err(|error| internal_error(error.into()))?;
                let updated = store
                    .update(&name, &trigger_type, &condition_json, &team_name, &input)
                    .map_err(|error| internal_error(error.into()))?;
                let mut page =
                    load_triggers_page(db.clone(), Some(name.clone())).map_err(internal_error)?;
                page.selected.notice = Some(Notice {
                    text: if updated.is_some() {
                        format!("Trigger `{name}` saved.")
                    } else {
                        format!("Trigger `{name}` no longer exists.")
                    },
                    tone: if updated.is_some() {
                        "success"
                    } else {
                        "danger"
                    },
                });
                page
            }
        }
        "toggle" => {
            match store
                .get_by_name(&target_name)
                .map_err(|error| internal_error(error.into()))?
            {
                Some(trigger) => {
                    store
                        .set_enabled(&target_name, !trigger.enabled)
                        .map_err(|error| internal_error(error.into()))?;
                    let mut page = load_triggers_page(db.clone(), Some(target_name.clone()))
                        .map_err(internal_error)?;
                    page.selected.notice = Some(Notice {
                        text: if trigger.enabled {
                            format!("Trigger `{target_name}` disabled.")
                        } else {
                            format!("Trigger `{target_name}` enabled.")
                        },
                        tone: "success",
                    });
                    page
                }
                None => {
                    let mut page = load_triggers_page(db.clone(), None).map_err(internal_error)?;
                    page.selected.notice = Some(Notice {
                        text: format!("Trigger `{target_name}` was not found."),
                        tone: "danger",
                    });
                    page
                }
            }
        }
        "delete" => {
            let removed = store
                .remove(&target_name)
                .map_err(|error| internal_error(error.into()))?;
            let mut page = load_triggers_page(db.clone(), None).map_err(internal_error)?;
            page.selected.notice = Some(Notice {
                text: if removed {
                    format!("Trigger `{target_name}` deleted.")
                } else {
                    format!("Trigger `{target_name}` was already removed.")
                },
                tone: if removed { "success" } else { "danger" },
            });
            page
        }
        "test" => {
            match store
                .get_by_name(&target_name)
                .map_err(|error| internal_error(error.into()))?
            {
                Some(trigger) => {
                    let trigger_name = trigger.name.clone();
                    let team_name = trigger.team_name.clone();
                    let db_for_run = db.clone();
                    let event_bus_for_run = event_bus.clone();
                    let input = if trigger.input.trim().is_empty() {
                        format!(
                            "Test run fired from the web dashboard for trigger {}",
                            trigger.name
                        )
                    } else {
                        trigger.input.clone()
                    };
                    tokio::spawn(async move {
                        match opengoose_teams::run_headless(
                            &team_name,
                            &input,
                            db_for_run.clone(),
                            event_bus_for_run,
                        )
                        .await
                        {
                            Ok(_) => {
                                let store = TriggerStore::new(db_for_run);
                                if let Err(error) = store.mark_fired(&trigger_name) {
                                    error!(trigger = %trigger_name, %error, "failed to mark trigger fired after page test");
                                }
                            }
                            Err(error) => {
                                error!(trigger = %trigger_name, team = %team_name, %error, "page trigger test failed");
                            }
                        }
                    });

                    let mut page = load_triggers_page(db.clone(), Some(target_name.clone()))
                        .map_err(internal_error)?;
                    page.selected.notice = Some(Notice {
                        text: format!(
                            "Trigger `{target_name}` test queued. Check Runs for progress."
                        ),
                        tone: "success",
                    });
                    page
                }
                None => {
                    let mut page = load_triggers_page(db.clone(), None).map_err(internal_error)?;
                    page.selected.notice = Some(Notice {
                        text: format!("Trigger `{target_name}` was not found."),
                        tone: "danger",
                    });
                    page
                }
            }
        }
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::response::Html("Unsupported trigger action.".into()),
            ));
        }
    };

    render_triggers_page(page)
}
