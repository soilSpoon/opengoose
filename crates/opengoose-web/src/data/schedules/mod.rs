mod catalog;
mod editor;
mod selection;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, ScheduleStore, ScheduleUpdate};
use opengoose_teams::scheduler;

use self::catalog::load_catalog;
use self::editor::{
    ScheduleDraft, build_error_page, build_page, normalize_input, normalize_optional_field,
    normalize_trimmed_field, trimmed_len_exceeds,
};
use self::selection::{NEW_SCHEDULE_KEY, resolve_selection};
use crate::data::views::{Notice, SchedulesPageView};

const MAX_SCHEDULE_NAME_BYTES: usize = 128;
const MAX_CRON_EXPRESSION_BYTES: usize = 128;
const MAX_TEAM_NAME_BYTES: usize = 128;
const MAX_SCHEDULE_INPUT_BYTES: usize = 8 * 1024;

pub struct ScheduleSaveInput {
    pub original_name: Option<String>,
    pub name: String,
    pub cron_expression: String,
    pub team_name: String,
    pub input: String,
    pub enabled: bool,
}

/// Load the schedules page view-model, optionally selecting a schedule by name.
pub fn load_schedules_page(
    db: Arc<Database>,
    selected: Option<String>,
) -> Result<SchedulesPageView> {
    let catalog = load_catalog(db)?;
    let selection = resolve_selection(&catalog, selected);
    build_page(&catalog, selection, None)
}

/// Create or update a schedule and return the refreshed page view.
pub fn save_schedule(db: Arc<Database>, input: ScheduleSaveInput) -> Result<SchedulesPageView> {
    let catalog = load_catalog(db.clone())?;
    let input_too_long = input.input.len() > MAX_SCHEDULE_INPUT_BYTES;
    let draft = ScheduleDraft {
        original_name: normalize_optional_field(
            input.original_name.as_deref(),
            MAX_SCHEDULE_NAME_BYTES,
        ),
        name: normalize_trimmed_field(&input.name, MAX_SCHEDULE_NAME_BYTES),
        cron_expression: normalize_trimmed_field(&input.cron_expression, MAX_CRON_EXPRESSION_BYTES),
        team_name: normalize_trimmed_field(&input.team_name, MAX_TEAM_NAME_BYTES),
        input: normalize_input(input.input, MAX_SCHEDULE_INPUT_BYTES),
        enabled: input.enabled,
    };

    if let Some(original_name) = input.original_name.as_deref()
        && trimmed_len_exceeds(original_name, MAX_SCHEDULE_NAME_BYTES)
    {
        return build_error_page(
            &catalog,
            &draft,
            format!(
                "Original schedule name must be {} bytes or less.",
                MAX_SCHEDULE_NAME_BYTES
            ),
        );
    }
    if trimmed_len_exceeds(&input.name, MAX_SCHEDULE_NAME_BYTES) {
        return build_error_page(
            &catalog,
            &draft,
            format!(
                "Schedule name must be {} bytes or less.",
                MAX_SCHEDULE_NAME_BYTES
            ),
        );
    }
    if trimmed_len_exceeds(&input.cron_expression, MAX_CRON_EXPRESSION_BYTES) {
        return build_error_page(
            &catalog,
            &draft,
            format!(
                "Cron expression must be {} bytes or less.",
                MAX_CRON_EXPRESSION_BYTES
            ),
        );
    }
    if trimmed_len_exceeds(&input.team_name, MAX_TEAM_NAME_BYTES) {
        return build_error_page(
            &catalog,
            &draft,
            format!("Team name must be {} bytes or less.", MAX_TEAM_NAME_BYTES),
        );
    }
    if input_too_long {
        return build_error_page(
            &catalog,
            &draft,
            format!(
                "Schedule input must be {} bytes or less.",
                MAX_SCHEDULE_INPUT_BYTES
            ),
        );
    }

    if draft.name.is_empty() {
        return build_error_page(&catalog, &draft, "Schedule name is required.");
    }
    if draft.cron_expression.is_empty() {
        return build_error_page(&catalog, &draft, "Cron expression is required.");
    }
    if draft.team_name.is_empty() {
        return build_error_page(&catalog, &draft, "Choose an installed team before saving.");
    }
    if !catalog
        .installed_teams
        .iter()
        .any(|team| team == &draft.team_name)
    {
        return build_error_page(
            &catalog,
            &draft,
            "The selected team is not installed. Save a team definition first.",
        );
    }
    if let Err(error) = scheduler::validate_cron(&draft.cron_expression) {
        return build_error_page(&catalog, &draft, error);
    }
    if let Some(original_name) = draft.original_name.as_ref()
        && draft.name != *original_name
    {
        return build_error_page(
            &catalog,
            &draft,
            "Schedule names are immutable. Create a new schedule instead.",
        );
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
