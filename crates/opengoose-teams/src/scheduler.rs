use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule as CronSchedule;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use opengoose_persistence::{Database, ScheduleStore};
use opengoose_types::EventBus;

/// Compute the next fire time for a cron expression after `after`.
pub fn next_fire_time(cron_expr: &str) -> Option<String> {
    let schedule = CronSchedule::from_str(cron_expr).ok()?;
    let next = schedule.upcoming(Utc).next()?;
    Some(next.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Validate a cron expression, returning an error message if invalid.
pub fn validate_cron(cron_expr: &str) -> Result<(), String> {
    CronSchedule::from_str(cron_expr)
        .map(|_| ())
        .map_err(|e| format!("invalid cron expression: {e}"))
}

/// Spawn the cron scheduler daemon as a background tokio task.
///
/// Checks for due schedules every 30 seconds and fires team runs.
pub fn spawn_scheduler(
    db: Arc<Database>,
    event_bus: EventBus,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("cron scheduler daemon started");

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("cron scheduler daemon stopped");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = tick(&db, &event_bus).await {
                        error!(%e, "scheduler tick failed");
                    }
                }
            }
        }
    })
}

async fn tick(db: &Arc<Database>, event_bus: &EventBus) -> anyhow::Result<()> {
    let store = ScheduleStore::new(db.clone());
    let due = store.list_due()?;

    for schedule in due {
        info!(
            name = %schedule.name,
            team = %schedule.team_name,
            "cron trigger: running team"
        );

        let input = if schedule.input.is_empty() {
            format!("Scheduled run: {}", schedule.name)
        } else {
            schedule.input.clone()
        };

        match crate::run_headless(&schedule.team_name, &input, db.clone(), event_bus.clone()).await
        {
            Ok((run_id, _result)) => {
                info!(
                    name = %schedule.name,
                    run_id = %run_id,
                    "scheduled team run completed"
                );
            }
            Err(e) => {
                warn!(
                    name = %schedule.name,
                    team = %schedule.team_name,
                    %e,
                    "scheduled team run failed"
                );
            }
        }

        // Update last_run_at and compute next_run_at
        let next = next_fire_time(&schedule.cron_expression);
        if let Err(e) = store.mark_run(&schedule.name, next.as_deref()) {
            error!(name = %schedule.name, %e, "failed to update schedule after run");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cron_valid() {
        assert!(validate_cron("0 0 * * * *").is_ok());
        assert!(validate_cron("0 30 9 * * Mon-Fri *").is_ok());
    }

    #[test]
    fn test_validate_cron_invalid() {
        assert!(validate_cron("not a cron").is_err());
        assert!(validate_cron("").is_err());
    }

    #[test]
    fn test_next_fire_time() {
        let next = next_fire_time("0 0 * * * *");
        assert!(next.is_some());
        // Should be a valid datetime string
        let s = next.unwrap();
        assert!(s.contains('-'));
        assert!(s.contains(':'));
    }

    #[test]
    fn test_next_fire_time_invalid() {
        assert!(next_fire_time("invalid").is_none());
    }

    #[test]
    fn test_next_fire_time_empty() {
        assert!(next_fire_time("").is_none());
    }

    #[test]
    fn test_validate_cron_every_minute() {
        assert!(validate_cron("0 * * * * *").is_ok());
    }

    #[test]
    fn test_validate_cron_error_message() {
        let err = validate_cron("not valid").unwrap_err();
        assert!(err.contains("invalid cron expression"));
    }

    #[test]
    fn test_next_fire_time_returns_datetime_format() {
        let next = next_fire_time("0 0 * * * *").unwrap();
        // Format: YYYY-MM-DD HH:MM:SS
        assert_eq!(next.len(), 19, "should be 19 chars: {next}");
        assert_eq!(&next[4..5], "-");
        assert_eq!(&next[7..8], "-");
        assert_eq!(&next[10..11], " ");
        assert_eq!(&next[13..14], ":");
        assert_eq!(&next[16..17], ":");
    }
}
