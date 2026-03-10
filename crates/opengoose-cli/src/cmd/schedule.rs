use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_persistence::{Database, ScheduleStore};
use opengoose_teams::scheduler;

#[derive(Subcommand)]
/// Subcommands for `opengoose schedule`.
pub enum ScheduleAction {
    /// Add a new cron schedule
    Add {
        /// Unique name for this schedule
        name: String,
        /// Cron expression (6-field: sec min hour day month weekday)
        #[arg(long)]
        cron: String,
        /// Team name to run
        #[arg(long)]
        team: String,
        /// Input text for the team (optional)
        #[arg(long, default_value = "")]
        input: String,
    },
    /// List all schedules
    List,
    /// Remove a schedule
    Remove {
        /// Schedule name
        name: String,
    },
    /// Enable a schedule
    Enable {
        /// Schedule name
        name: String,
    },
    /// Disable a schedule
    Disable {
        /// Schedule name
        name: String,
    },
    /// Show status of a specific schedule
    Status {
        /// Schedule name
        name: String,
    },
}

/// Dispatch and execute the selected schedule subcommand.
pub fn execute(action: ScheduleAction) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let team_store = opengoose_teams::TeamStore::new()?;
    run(action, db, &team_store)
}

/// Testable dispatch: accepts injected db and team_store.
pub(crate) fn run(
    action: ScheduleAction,
    db: Arc<Database>,
    team_store: &opengoose_teams::TeamStore,
) -> Result<()> {
    let store = ScheduleStore::new(db);
    match action {
        ScheduleAction::Add {
            name,
            cron,
            team,
            input,
        } => cmd_add(&store, team_store, &name, &cron, &team, &input),
        ScheduleAction::List => cmd_list(&store),
        ScheduleAction::Remove { name } => cmd_remove(&store, &name),
        ScheduleAction::Enable { name } => cmd_enable(&store, &name),
        ScheduleAction::Disable { name } => cmd_disable(&store, &name),
        ScheduleAction::Status { name } => cmd_status(&store, &name),
    }
}

fn cmd_add(
    store: &ScheduleStore,
    team_store: &opengoose_teams::TeamStore,
    name: &str,
    cron_expr: &str,
    team: &str,
    input: &str,
) -> Result<()> {
    // Validate the cron expression
    scheduler::validate_cron(cron_expr).map_err(|e| anyhow::anyhow!(e))?;

    // Verify team exists
    if team_store.get(team).is_err() {
        bail!(
            "team '{}' not found. Use `opengoose team list` to see available teams.",
            team
        );
    }

    let next = scheduler::next_fire_time(cron_expr);
    let schedule = store.create(name, cron_expr, team, input, next.as_deref())?;

    println!("Created schedule '{}'.", schedule.name);
    println!("  Team: {}", schedule.team_name);
    println!("  Cron: {}", schedule.cron_expression);
    if let Some(ref next_run) = schedule.next_run_at {
        println!("  Next run: {next_run}");
    }

    Ok(())
}

fn cmd_list(store: &ScheduleStore) -> Result<()> {
    let schedules = store.list()?;

    if schedules.is_empty() {
        println!("No schedules found. Use `opengoose schedule add` to create one.");
        return Ok(());
    }

    println!(
        "{:<20} {:<10} {:<20} {:<20} {:<20}",
        "NAME", "ENABLED", "CRON", "TEAM", "NEXT RUN"
    );
    for s in &schedules {
        let enabled = if s.enabled { "yes" } else { "no" };
        let next = s.next_run_at.as_deref().unwrap_or("-");
        println!(
            "{:<20} {:<10} {:<20} {:<20} {:<20}",
            s.name, enabled, s.cron_expression, s.team_name, next
        );
    }

    Ok(())
}

fn cmd_remove(store: &ScheduleStore, name: &str) -> Result<()> {
    if store.remove(name)? {
        println!("Removed schedule '{name}'.");
    } else {
        bail!("schedule '{name}' not found.");
    }

    Ok(())
}

fn cmd_enable(store: &ScheduleStore, name: &str) -> Result<()> {
    // Recompute next_run_at when enabling
    if let Some(schedule) = store.get_by_name(name)? {
        let next = scheduler::next_fire_time(&schedule.cron_expression);
        store.mark_run(name, next.as_deref())?;
    }

    if store.set_enabled(name, true)? {
        println!("Enabled schedule '{name}'.");
    } else {
        bail!("schedule '{name}' not found.");
    }

    Ok(())
}

fn cmd_disable(store: &ScheduleStore, name: &str) -> Result<()> {
    if store.set_enabled(name, false)? {
        println!("Disabled schedule '{name}'.");
    } else {
        bail!("schedule '{name}' not found.");
    }

    Ok(())
}

fn cmd_status(store: &ScheduleStore, name: &str) -> Result<()> {
    let schedule = store
        .get_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("schedule '{}' not found", name))?;

    println!("Schedule: {}", schedule.name);
    println!("  Team: {}", schedule.team_name);
    println!("  Cron: {}", schedule.cron_expression);
    println!("  Enabled: {}", if schedule.enabled { "yes" } else { "no" });
    println!(
        "  Last run: {}",
        schedule.last_run_at.as_deref().unwrap_or("never")
    );
    println!(
        "  Next run: {}",
        schedule.next_run_at.as_deref().unwrap_or("-")
    );
    if !schedule.input.is_empty() {
        let preview = if schedule.input.len() > 100 {
            format!(
                "{}...",
                &schedule.input[..schedule.input.floor_char_boundary(100)]
            )
        } else {
            schedule.input.clone()
        };
        println!("  Input: {preview}");
    }
    println!("  Created: {}", schedule.created_at);
    println!("  Updated: {}", schedule.updated_at);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> ScheduleStore {
        let db = Arc::new(opengoose_persistence::Database::open_in_memory().unwrap());
        ScheduleStore::new(db)
    }

    fn make_db() -> Arc<Database> {
        Arc::new(opengoose_persistence::Database::open_in_memory().unwrap())
    }

    /// Create a temporary TeamStore with a named team YAML file.
    fn make_team_store_with(team_name: &str) -> (tempfile::TempDir, opengoose_teams::TeamStore) {
        let dir = tempfile::tempdir().unwrap();
        let yaml = format!(
            "version: \"1.0\"\ntitle: {team_name}\nworkflow: chain\nagents:\n  - profile: default\n"
        );
        std::fs::write(dir.path().join(format!("{team_name}.yaml")), yaml).unwrap();
        let store = opengoose_teams::TeamStore::with_dir(dir.path().to_path_buf());
        (dir, store)
    }

    fn empty_team_store() -> (tempfile::TempDir, opengoose_teams::TeamStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = opengoose_teams::TeamStore::with_dir(dir.path().to_path_buf());
        (dir, store)
    }

    // ---- validate_cron ----

    #[test]
    fn validate_cron_accepts_standard_six_field_expression() {
        // sec min hour day month weekday
        assert!(scheduler::validate_cron("0 0 * * * *").is_ok());
    }

    #[test]
    fn validate_cron_accepts_every_minute() {
        assert!(scheduler::validate_cron("0 * * * * *").is_ok());
    }

    #[test]
    fn validate_cron_accepts_specific_time() {
        assert!(scheduler::validate_cron("0 30 9 * * *").is_ok());
    }

    #[test]
    fn validate_cron_rejects_empty_string() {
        assert!(scheduler::validate_cron("").is_err());
    }

    #[test]
    fn validate_cron_rejects_invalid_expression() {
        let err = scheduler::validate_cron("not-a-cron").unwrap_err();
        assert!(err.contains("invalid cron expression"));
    }

    #[test]
    fn validate_cron_rejects_too_few_fields() {
        assert!(scheduler::validate_cron("* * *").is_err());
    }

    #[test]
    fn next_fire_time_returns_some_for_valid_expression() {
        let result = scheduler::next_fire_time("0 * * * * *");
        assert!(result.is_some());
        let time_str = result.unwrap();
        // Should be a date string
        assert!(time_str.contains('-'));
        assert!(time_str.contains(':'));
    }

    #[test]
    fn next_fire_time_returns_none_for_invalid_expression() {
        let result = scheduler::next_fire_time("invalid");
        assert!(result.is_none());
    }

    // ---- ScheduleStore with in-memory DB ----

    #[test]
    fn schedule_store_list_empty_initially() {
        let store = make_store();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn schedule_store_create_and_list() {
        let store = make_store();
        let sched = store
            .create("daily", "0 0 8 * * *", "my-team", "", None)
            .unwrap();
        assert_eq!(sched.name, "daily");
        assert_eq!(sched.cron_expression, "0 0 8 * * *");
        assert_eq!(sched.team_name, "my-team");
        assert!(sched.enabled);

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn schedule_store_get_by_name_returns_correct_schedule() {
        let store = make_store();
        store
            .create("alpha", "0 0 * * * *", "team-a", "run report", None)
            .unwrap();
        store
            .create("beta", "0 30 * * * *", "team-b", "", None)
            .unwrap();

        let found = store.get_by_name("alpha").unwrap().unwrap();
        assert_eq!(found.name, "alpha");
        assert_eq!(found.input, "run report");
    }

    #[test]
    fn schedule_store_get_by_name_returns_none_for_missing() {
        let store = make_store();
        assert!(store.get_by_name("missing").unwrap().is_none());
    }

    #[test]
    fn schedule_store_remove_existing_returns_true() {
        let store = make_store();
        store
            .create("to-remove", "0 * * * * *", "team", "", None)
            .unwrap();
        assert!(store.remove("to-remove").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn schedule_store_remove_nonexistent_returns_false() {
        let store = make_store();
        assert!(!store.remove("ghost").unwrap());
    }

    #[test]
    fn schedule_store_set_enabled_toggle() {
        let store = make_store();
        store
            .create("toggle", "0 * * * * *", "team", "", None)
            .unwrap();

        assert!(store.set_enabled("toggle", false).unwrap());
        let s = store.get_by_name("toggle").unwrap().unwrap();
        assert!(!s.enabled);

        assert!(store.set_enabled("toggle", true).unwrap());
        let s = store.get_by_name("toggle").unwrap().unwrap();
        assert!(s.enabled);
    }

    #[test]
    fn schedule_store_set_enabled_nonexistent_returns_false() {
        let store = make_store();
        assert!(!store.set_enabled("nonexistent", true).unwrap());
    }

    #[test]
    fn schedule_store_create_with_next_run_at() {
        let store = make_store();
        let next = "2030-01-01 00:00:00";
        let sched = store
            .create("future", "0 0 0 1 1 *", "team", "", Some(next))
            .unwrap();
        assert_eq!(sched.next_run_at.as_deref(), Some(next));
    }

    #[test]
    fn schedule_store_mark_run_updates_next_run_at() {
        let store = make_store();
        store
            .create("runner", "0 * * * * *", "team", "", None)
            .unwrap();

        let new_next = "2030-06-15 12:00:00";
        assert!(store.mark_run("runner", Some(new_next)).unwrap());

        let s = store.get_by_name("runner").unwrap().unwrap();
        assert_eq!(s.next_run_at.as_deref(), Some(new_next));
    }

    #[test]
    fn schedule_store_input_preserved() {
        let store = make_store();
        let input = "analyze sales data for Q4";
        store
            .create("report", "0 0 9 * * 1", "team", input, None)
            .unwrap();

        let s = store.get_by_name("report").unwrap().unwrap();
        assert_eq!(s.input, input);
    }

    // ---- CLI dispatch path tests via run() ----

    #[test]
    fn dispatch_list_empty_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();
        let result = run(ScheduleAction::List, db, &team_store);
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_add_creates_schedule_in_db() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("my-team");

        let result = run(
            ScheduleAction::Add {
                name: "nightly".to_string(),
                cron: "0 0 2 * * *".to_string(),
                team: "my-team".to_string(),
                input: "run nightly".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok(), "add should succeed: {result:?}");

        // Verify schedule was persisted
        let store = ScheduleStore::new(db);
        let s = store.get_by_name("nightly").unwrap().unwrap();
        assert_eq!(s.team_name, "my-team");
        assert_eq!(s.cron_expression, "0 0 2 * * *");
        assert_eq!(s.input, "run nightly");
        assert!(s.enabled);
    }

    #[test]
    fn dispatch_add_rejects_invalid_cron() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("my-team");

        let result = run(
            ScheduleAction::Add {
                name: "bad".to_string(),
                cron: "not-a-cron".to_string(),
                team: "my-team".to_string(),
                input: "".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid cron"));
    }

    #[test]
    fn dispatch_add_rejects_nonexistent_team() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            ScheduleAction::Add {
                name: "sched".to_string(),
                cron: "0 * * * * *".to_string(),
                team: "no-such-team".to_string(),
                input: "".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("no-such-team"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn dispatch_list_shows_created_schedule() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("ops-team");

        run(
            ScheduleAction::Add {
                name: "weekly".to_string(),
                cron: "0 0 9 * * 1".to_string(),
                team: "ops-team".to_string(),
                input: "".to_string(),
            },
            db.clone(),
            &team_store,
        )
        .unwrap();

        // List should succeed (output goes to stdout, we just verify no error)
        let result = run(ScheduleAction::List, db, &team_store);
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_remove_existing_schedule_succeeds() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("team-a");

        // Seed a schedule directly in the store
        ScheduleStore::new(db.clone())
            .create("to-delete", "0 * * * * *", "team-a", "", None)
            .unwrap();

        let result = run(
            ScheduleAction::Remove {
                name: "to-delete".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok());

        // Verify removed
        let store = ScheduleStore::new(db);
        assert!(store.get_by_name("to-delete").unwrap().is_none());
    }

    #[test]
    fn dispatch_remove_nonexistent_schedule_errors() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            ScheduleAction::Remove {
                name: "ghost".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_enable_existing_schedule_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        // Create a disabled schedule
        let store = ScheduleStore::new(db.clone());
        store
            .create("sched", "0 * * * * *", "team", "", None)
            .unwrap();
        store.set_enabled("sched", false).unwrap();

        let result = run(
            ScheduleAction::Enable {
                name: "sched".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok());

        let s = ScheduleStore::new(db).get_by_name("sched").unwrap().unwrap();
        assert!(s.enabled);
    }

    #[test]
    fn dispatch_enable_nonexistent_errors() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            ScheduleAction::Enable {
                name: "no-such".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_disable_existing_schedule_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        ScheduleStore::new(db.clone())
            .create("active", "0 * * * * *", "team", "", None)
            .unwrap();

        let result = run(
            ScheduleAction::Disable {
                name: "active".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok());

        let s = ScheduleStore::new(db)
            .get_by_name("active")
            .unwrap()
            .unwrap();
        assert!(!s.enabled);
    }

    #[test]
    fn dispatch_disable_nonexistent_errors() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            ScheduleAction::Disable {
                name: "no-such".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_status_existing_schedule_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        ScheduleStore::new(db.clone())
            .create("my-sched", "0 0 8 * * *", "my-team", "hello", None)
            .unwrap();

        let result = run(
            ScheduleAction::Status {
                name: "my-sched".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_status_nonexistent_errors() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            ScheduleAction::Status {
                name: "missing".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_add_then_disable_then_enable_roundtrip() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("ops");

        run(
            ScheduleAction::Add {
                name: "daily".to_string(),
                cron: "0 0 8 * * *".to_string(),
                team: "ops".to_string(),
                input: "".to_string(),
            },
            db.clone(),
            &team_store,
        )
        .unwrap();

        run(
            ScheduleAction::Disable {
                name: "daily".to_string(),
            },
            db.clone(),
            &team_store,
        )
        .unwrap();

        let s = ScheduleStore::new(db.clone())
            .get_by_name("daily")
            .unwrap()
            .unwrap();
        assert!(!s.enabled);

        run(
            ScheduleAction::Enable {
                name: "daily".to_string(),
            },
            db.clone(),
            &team_store,
        )
        .unwrap();

        let s = ScheduleStore::new(db).get_by_name("daily").unwrap().unwrap();
        assert!(s.enabled);
    }

    #[test]
    fn dispatch_add_then_remove_clears_db() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("ops");

        run(
            ScheduleAction::Add {
                name: "tmp".to_string(),
                cron: "0 * * * * *".to_string(),
                team: "ops".to_string(),
                input: "".to_string(),
            },
            db.clone(),
            &team_store,
        )
        .unwrap();

        run(
            ScheduleAction::Remove {
                name: "tmp".to_string(),
            },
            db.clone(),
            &team_store,
        )
        .unwrap();

        assert!(ScheduleStore::new(db).list().unwrap().is_empty());
    }

    #[test]
    fn dispatch_add_with_empty_input_succeeds() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("silent");

        let result = run(
            ScheduleAction::Add {
                name: "no-input".to_string(),
                cron: "0 0 0 * * *".to_string(),
                team: "silent".to_string(),
                input: "".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok());

        let s = ScheduleStore::new(db)
            .get_by_name("no-input")
            .unwrap()
            .unwrap();
        assert_eq!(s.input, "");
    }
}
