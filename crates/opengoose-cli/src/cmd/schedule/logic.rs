use crate::error::{CliError, CliResult};

pub(super) fn validate_cron_expression(cron_expr: &str) -> CliResult<()> {
    opengoose_teams::scheduler::validate_cron(cron_expr)
        .map_err(|err| CliError::Validation(err.to_string()))
}

pub(super) fn ensure_team_exists(
    team_store: &opengoose_teams::TeamStore,
    team: &str,
) -> CliResult<()> {
    if team_store.get(team).is_err() {
        return Err(CliError::Validation(format!(
            "team '{}' not found. Use `opengoose team list` to see available teams.",
            team
        )));
    }

    Ok(())
}

pub(super) fn next_run_at(cron_expr: &str) -> Option<String> {
    opengoose_teams::scheduler::next_fire_time(cron_expr)
}

pub(super) fn preview_input(input: &str) -> String {
    if input.len() > 100 {
        format!("{}...", &input[..input.floor_char_boundary(100)])
    } else {
        input.to_owned()
    }
}
