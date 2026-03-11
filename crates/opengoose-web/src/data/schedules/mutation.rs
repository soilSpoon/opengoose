use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, ScheduleStore, ScheduleUpdate};
use opengoose_teams::scheduler;

use crate::data::views::{Notice, SchedulesPageView};

use super::query::load_schedules_page;
use super::shared::{NEW_SCHEDULE_KEY, ScheduleSaveInput, load_catalog};
use super::validation::{build_draft, validate_schedule_draft};
use super::view::build_error_page;

/// Create or update a schedule and return the refreshed page view.
pub fn save_schedule(db: Arc<Database>, input: ScheduleSaveInput) -> Result<SchedulesPageView> {
    let catalog = load_catalog(db.clone())?;
    let draft = build_draft(input);

    if let Err(message) = validate_schedule_draft(&catalog, &draft) {
        return build_error_page(&catalog, &draft, message);
    }

    let next_run_at = if draft.enabled {
        scheduler::next_fire_time(&draft.cron_expression)
    } else {
        None
    };
    let store = ScheduleStore::new(db.clone());

    let result = if let Some(original_name) = draft.original_name.as_ref() {
        store.update(
            original_name,
            ScheduleUpdate {
                name: &draft.name,
                cron_expression: &draft.cron_expression,
                team_name: &draft.team_name,
                input: &draft.input,
                enabled: draft.enabled,
                next_run_at: next_run_at.as_deref(),
            },
        )
    } else {
        store
            .create(
                &draft.name,
                &draft.cron_expression,
                &draft.team_name,
                &draft.input,
                next_run_at.as_deref(),
            )
            .map(|_| true)
    };

    match result {
        Ok(true) => {
            let mut page = load_schedules_page(db, Some(draft.name.clone()))?;
            page.selected.notice = Some(Notice {
                text: if draft.original_name.is_some() {
                    "Schedule saved.".into()
                } else {
                    "Schedule created.".into()
                },
                tone: "success",
            });
            Ok(page)
        }
        Ok(false) => build_error_page(
            &catalog,
            &draft,
            "The selected schedule no longer exists. Reload and try again.",
        ),
        Err(error) => build_error_page(&catalog, &draft, error.to_string()),
    }
}

/// Flip a schedule between enabled and paused states.
pub fn toggle_schedule(db: Arc<Database>, name: String) -> Result<SchedulesPageView> {
    let store = ScheduleStore::new(db.clone());
    let Some(schedule) = store.get_by_name(&name)? else {
        let mut page = load_schedules_page(db, Some(NEW_SCHEDULE_KEY.into()))?;
        page.selected.notice = Some(Notice {
            text: format!("Schedule `{name}` was not found."),
            tone: "danger",
        });
        return Ok(page);
    };

    let enabled = !schedule.enabled;
    if enabled && let Err(error) = scheduler::validate_cron(&schedule.cron_expression) {
        let mut page = load_schedules_page(db, Some(schedule.name.clone()))?;
        page.selected.notice = Some(Notice {
            text: error,
            tone: "danger",
        });
        return Ok(page);
    }

    let next_run_at = if enabled {
        scheduler::next_fire_time(&schedule.cron_expression)
    } else {
        None
    };
    store.update(
        &schedule.name,
        ScheduleUpdate {
            name: &schedule.name,
            cron_expression: &schedule.cron_expression,
            team_name: &schedule.team_name,
            input: &schedule.input,
            enabled,
            next_run_at: next_run_at.as_deref(),
        },
    )?;

    let mut page = load_schedules_page(db, Some(schedule.name.clone()))?;
    page.selected.notice = Some(Notice {
        text: if enabled {
            "Schedule enabled.".into()
        } else {
            "Schedule paused.".into()
        },
        tone: "success",
    });
    Ok(page)
}

/// Delete a schedule after explicit confirmation.
pub fn delete_schedule(
    db: Arc<Database>,
    name: String,
    confirmed: bool,
) -> Result<SchedulesPageView> {
    if !confirmed {
        let mut page = load_schedules_page(db, Some(name.clone()))?;
        page.selected.notice = Some(Notice {
            text: "Check the confirmation box before deleting a schedule.".into(),
            tone: "danger",
        });
        return Ok(page);
    }

    let removed = ScheduleStore::new(db.clone()).remove(&name)?;
    let mut page = load_schedules_page(db, None)?;
    page.selected.notice = Some(Notice {
        text: if removed {
            format!("Deleted schedule `{name}`.")
        } else {
            format!("Schedule `{name}` was already removed.")
        },
        tone: if removed { "success" } else { "danger" },
    });
    Ok(page)
}
