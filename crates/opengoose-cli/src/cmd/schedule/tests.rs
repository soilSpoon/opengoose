use std::sync::Arc;

use opengoose_persistence::{Database, ScheduleStore};
use opengoose_teams::scheduler;

use super::{ScheduleAction, run};

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

#[test]
fn validate_cron_accepts_standard_six_field_expression() {
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
    assert!(time_str.contains('-'));
    assert!(time_str.contains(':'));
}

#[test]
fn next_fire_time_returns_none_for_invalid_expression() {
    let result = scheduler::next_fire_time("invalid");
    assert!(result.is_none());
}

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
    let schedule = store.get_by_name("toggle").unwrap().unwrap();
    assert!(!schedule.enabled);

    assert!(store.set_enabled("toggle", true).unwrap());
    let schedule = store.get_by_name("toggle").unwrap().unwrap();
    assert!(schedule.enabled);
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

    let schedule = store.get_by_name("runner").unwrap().unwrap();
    assert_eq!(schedule.next_run_at.as_deref(), Some(new_next));
}

#[test]
fn schedule_store_input_preserved() {
    let store = make_store();
    let input = "analyze sales data for Q4";
    store
        .create("report", "0 0 9 * * 1", "team", input, None)
        .unwrap();

    let schedule = store.get_by_name("report").unwrap().unwrap();
    assert_eq!(schedule.input, input);
}

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

    let schedule = ScheduleStore::new(db)
        .get_by_name("nightly")
        .unwrap()
        .unwrap();
    assert_eq!(schedule.team_name, "my-team");
    assert_eq!(schedule.cron_expression, "0 0 2 * * *");
    assert_eq!(schedule.input, "run nightly");
    assert!(schedule.enabled);
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

    let result = run(ScheduleAction::List, db, &team_store);
    assert!(result.is_ok());
}

#[test]
fn dispatch_remove_existing_schedule_succeeds() {
    let db = make_db();
    let (_dir, team_store) = make_team_store_with("team-a");

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

    let schedule = ScheduleStore::new(db)
        .get_by_name("sched")
        .unwrap()
        .unwrap();
    assert!(schedule.enabled);
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

    let schedule = ScheduleStore::new(db)
        .get_by_name("active")
        .unwrap()
        .unwrap();
    assert!(!schedule.enabled);
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

    let schedule = ScheduleStore::new(db.clone())
        .get_by_name("daily")
        .unwrap()
        .unwrap();
    assert!(!schedule.enabled);

    run(
        ScheduleAction::Enable {
            name: "daily".to_string(),
        },
        db.clone(),
        &team_store,
    )
    .unwrap();

    let schedule = ScheduleStore::new(db)
        .get_by_name("daily")
        .unwrap()
        .unwrap();
    assert!(schedule.enabled);
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

    let schedule = ScheduleStore::new(db)
        .get_by_name("no-input")
        .unwrap()
        .unwrap();
    assert_eq!(schedule.input, "");
}
