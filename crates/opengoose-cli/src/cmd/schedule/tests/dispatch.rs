use opengoose_persistence::ScheduleStore;

use super::super::{run, ScheduleAction};
use super::support::{empty_team_store, make_db, make_team_store_with};

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
