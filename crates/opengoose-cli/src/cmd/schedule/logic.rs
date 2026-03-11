use anyhow::{Result, bail};

pub(super) fn validate_cron_expression(cron_expr: &str) -> Result<()> {
    opengoose_teams::scheduler::validate_cron(cron_expr).map_err(|err| anyhow::anyhow!(err))
}

pub(super) fn ensure_team_exists(
    team_store: &opengoose_teams::TeamStore,
    team: &str,
) -> Result<()> {
    if team_store.get(team).is_err() {
        bail!(
            "team '{}' not found. Use `opengoose team list` to see available teams.",
            team
        );
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
