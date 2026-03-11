use opengoose_teams::scheduler;

use super::shared::{ScheduleCatalog, ScheduleDraft, ScheduleSaveInput};

pub(super) fn build_draft(input: ScheduleSaveInput) -> ScheduleDraft {
    ScheduleDraft {
        original_name: input
            .original_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        name: input.name.trim().to_string(),
        cron_expression: input.cron_expression.trim().to_string(),
        team_name: input.team_name.trim().to_string(),
        input: normalize_input(input.input),
        enabled: input.enabled,
    }
}

pub(super) fn validate_schedule_draft(
    catalog: &ScheduleCatalog,
    draft: &ScheduleDraft,
) -> Result<(), String> {
    if draft.name.is_empty() {
        return Err("Schedule name is required.".into());
    }
    if draft.cron_expression.is_empty() {
        return Err("Cron expression is required.".into());
    }
    if draft.team_name.is_empty() {
        return Err("Choose an installed team before saving.".into());
    }
    if !catalog
        .installed_teams
        .iter()
        .any(|team| team == &draft.team_name)
    {
        return Err("The selected team is not installed. Save a team definition first.".into());
    }
    if let Err(error) = scheduler::validate_cron(&draft.cron_expression) {
        return Err(error);
    }
    if let Some(original_name) = draft.original_name.as_ref()
        && draft.name != *original_name
    {
        return Err("Schedule names are immutable. Create a new schedule instead.".into());
    }

    Ok(())
}

pub(super) fn normalize_input(input: String) -> String {
    if input.trim().is_empty() {
        String::new()
    } else {
        input
    }
}
