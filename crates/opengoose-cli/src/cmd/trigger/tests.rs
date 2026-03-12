use std::sync::Arc;

use opengoose_persistence::{Database, TriggerStore};

use super::{TriggerAction, logic, run};

fn make_store() -> TriggerStore {
    let db = Arc::new(opengoose_persistence::Database::open_in_memory().unwrap());
    TriggerStore::new(db)
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
fn validate_trigger_type_accepts_file_watch() {
    assert!(logic::validate_trigger_type("file_watch").is_ok());
}

#[test]
fn validate_trigger_type_accepts_message_received() {
    assert!(logic::validate_trigger_type("message_received").is_ok());
}

#[test]
fn validate_trigger_type_accepts_schedule_complete() {
    assert!(logic::validate_trigger_type("schedule_complete").is_ok());
}

#[test]
fn validate_trigger_type_accepts_webhook_received() {
    assert!(logic::validate_trigger_type("webhook_received").is_ok());
}

#[test]
fn validate_trigger_type_rejects_invalid() {
    let err = logic::validate_trigger_type("kafka_event").unwrap_err();
    assert!(err.to_string().contains("kafka_event"));
}

#[test]
fn validate_trigger_type_rejects_empty_string() {
    assert!(logic::validate_trigger_type("").is_err());
}

#[test]
fn condition_json_valid_object() {
    assert!(logic::validate_condition_json(r#"{"channel":"alerts"}"#).is_ok());
}

#[test]
fn condition_json_empty_object_is_valid() {
    assert!(logic::validate_condition_json("{}").is_ok());
}

#[test]
fn condition_json_invalid_returns_error() {
    assert!(logic::validate_condition_json("not json").is_err());
}

#[test]
fn preview_input_truncates_on_char_boundary() {
    let input = "한".repeat(40);
    assert_eq!(
        logic::preview_input(&input),
        format!("{}...", "한".repeat(33))
    );
}

#[test]
fn trigger_store_list_empty_initially() {
    let store = make_store();
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn trigger_store_create_and_list() {
    let store = make_store();
    let trigger = store
        .create("my-trigger", "file_watch", "{}", "my-team", "")
        .unwrap();
    assert_eq!(trigger.name, "my-trigger");
    assert_eq!(trigger.trigger_type, "file_watch");
    assert_eq!(trigger.team_name, "my-team");
    assert!(trigger.enabled);
    assert_eq!(trigger.fire_count, 0);

    let list = store.list().unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn trigger_store_get_by_name_returns_correct_trigger() {
    let store = make_store();
    store
        .create("alpha", "webhook_received", "{}", "team-a", "hello")
        .unwrap();
    store
        .create("beta", "message_received", "{}", "team-b", "")
        .unwrap();

    let found = store.get_by_name("alpha").unwrap().unwrap();
    assert_eq!(found.name, "alpha");
    assert_eq!(found.input, "hello");
}

#[test]
fn trigger_store_get_by_name_returns_none_for_missing() {
    let store = make_store();
    assert!(store.get_by_name("missing").unwrap().is_none());
}

#[test]
fn trigger_store_remove_existing_returns_true() {
    let store = make_store();
    store
        .create("to-remove", "file_watch", "{}", "team", "")
        .unwrap();
    assert!(store.remove("to-remove").unwrap());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn trigger_store_remove_nonexistent_returns_false() {
    let store = make_store();
    assert!(!store.remove("ghost").unwrap());
}

#[test]
fn trigger_store_set_enabled_disable_and_re_enable() {
    let store = make_store();
    store
        .create("toggle", "file_watch", "{}", "team", "")
        .unwrap();

    assert!(store.set_enabled("toggle", false).unwrap());
    let trigger = store.get_by_name("toggle").unwrap().unwrap();
    assert!(!trigger.enabled);

    assert!(store.set_enabled("toggle", true).unwrap());
    let trigger = store.get_by_name("toggle").unwrap().unwrap();
    assert!(trigger.enabled);
}

#[test]
fn trigger_store_set_enabled_nonexistent_returns_false() {
    let store = make_store();
    assert!(!store.set_enabled("nonexistent", false).unwrap());
}

#[test]
fn trigger_store_mark_fired_increments_count() {
    let store = make_store();
    store
        .create("fire-me", "webhook_received", "{}", "team", "")
        .unwrap();

    store.mark_fired("fire-me").unwrap();
    let trigger = store.get_by_name("fire-me").unwrap().unwrap();
    assert_eq!(trigger.fire_count, 1);
    assert!(trigger.last_fired_at.is_some());
}

#[test]
fn trigger_store_condition_json_stored_and_retrieved() {
    let store = make_store();
    let condition = r#"{"channel":"general","user":"alice"}"#;
    store
        .create("cond-trigger", "message_received", condition, "team", "")
        .unwrap();

    let trigger = store.get_by_name("cond-trigger").unwrap().unwrap();
    assert_eq!(trigger.condition_json, condition);
}

#[test]
fn trigger_store_list_by_type_filters_correctly() {
    let store = make_store();
    store.create("t1", "file_watch", "{}", "team", "").unwrap();
    store
        .create("t2", "webhook_received", "{}", "team", "")
        .unwrap();
    store.create("t3", "file_watch", "{}", "team", "").unwrap();

    let file_watch = store.list_by_type("file_watch").unwrap();
    assert_eq!(file_watch.len(), 2);
    let webhook = store.list_by_type("webhook_received").unwrap();
    assert_eq!(webhook.len(), 1);
}

#[test]
fn dispatch_list_empty_succeeds() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();
    let result = run(TriggerAction::List, db, &team_store);
    assert!(result.is_ok());
}

#[test]
fn dispatch_add_creates_trigger_in_db() {
    let db = make_db();
    let (_dir, team_store) = make_team_store_with("alert-team");

    let result = run(
        TriggerAction::Add {
            name: "on-webhook".to_string(),
            trigger_type: "webhook_received".to_string(),
            team: "alert-team".to_string(),
            condition: r#"{"path":"/github/pr"}"#.to_string(),
            input: "handle PR event".to_string(),
        },
        db.clone(),
        &team_store,
    );
    assert!(result.is_ok(), "add should succeed: {result:?}");

    let store = TriggerStore::new(db);
    let trigger = store.get_by_name("on-webhook").unwrap().unwrap();
    assert_eq!(trigger.trigger_type, "webhook_received");
    assert_eq!(trigger.team_name, "alert-team");
    assert_eq!(trigger.input, "handle PR event");
    assert!(trigger.enabled);
}

#[test]
fn dispatch_add_rejects_invalid_trigger_type() {
    let db = make_db();
    let (_dir, team_store) = make_team_store_with("my-team");

    let result = run(
        TriggerAction::Add {
            name: "bad-type".to_string(),
            trigger_type: "kafka_event".to_string(),
            team: "my-team".to_string(),
            condition: "{}".to_string(),
            input: "".to_string(),
        },
        db,
        &team_store,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("kafka_event"));
}

#[test]
fn dispatch_add_rejects_invalid_condition_json() {
    let db = make_db();
    let (_dir, team_store) = make_team_store_with("my-team");

    let result = run(
        TriggerAction::Add {
            name: "bad-json".to_string(),
            trigger_type: "file_watch".to_string(),
            team: "my-team".to_string(),
            condition: "not-json".to_string(),
            input: "".to_string(),
        },
        db,
        &team_store,
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("invalid condition JSON")
    );
}

#[test]
fn dispatch_add_rejects_nonexistent_team() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();

    let result = run(
        TriggerAction::Add {
            name: "t".to_string(),
            trigger_type: "file_watch".to_string(),
            team: "missing-team".to_string(),
            condition: "{}".to_string(),
            input: "".to_string(),
        },
        db,
        &team_store,
    );
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("missing-team"));
    assert!(msg.contains("not found"));
}

#[test]
fn dispatch_remove_existing_trigger_succeeds() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();

    TriggerStore::new(db.clone())
        .create("to-remove", "file_watch", "{}", "team", "")
        .unwrap();

    let result = run(
        TriggerAction::Remove {
            name: "to-remove".to_string(),
        },
        db.clone(),
        &team_store,
    );
    assert!(result.is_ok());

    assert!(
        TriggerStore::new(db)
            .get_by_name("to-remove")
            .unwrap()
            .is_none()
    );
}

#[test]
fn dispatch_remove_nonexistent_trigger_errors() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();

    let result = run(
        TriggerAction::Remove {
            name: "ghost".to_string(),
        },
        db,
        &team_store,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn dispatch_enable_trigger_succeeds() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();

    let store = TriggerStore::new(db.clone());
    store.create("t", "file_watch", "{}", "team", "").unwrap();
    store.set_enabled("t", false).unwrap();

    let result = run(
        TriggerAction::Enable {
            name: "t".to_string(),
        },
        db.clone(),
        &team_store,
    );
    assert!(result.is_ok());

    let trigger = TriggerStore::new(db).get_by_name("t").unwrap().unwrap();
    assert!(trigger.enabled);
}

#[test]
fn dispatch_enable_nonexistent_errors() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();

    let result = run(
        TriggerAction::Enable {
            name: "no-such".to_string(),
        },
        db,
        &team_store,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn dispatch_disable_trigger_succeeds() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();

    TriggerStore::new(db.clone())
        .create("active", "webhook_received", "{}", "team", "")
        .unwrap();

    let result = run(
        TriggerAction::Disable {
            name: "active".to_string(),
        },
        db.clone(),
        &team_store,
    );
    assert!(result.is_ok());

    let trigger = TriggerStore::new(db)
        .get_by_name("active")
        .unwrap()
        .unwrap();
    assert!(!trigger.enabled);
}

#[test]
fn dispatch_disable_nonexistent_errors() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();

    let result = run(
        TriggerAction::Disable {
            name: "no-such".to_string(),
        },
        db,
        &team_store,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn dispatch_status_existing_trigger_succeeds() {
    let db = make_db();
    let (_dir, team_store) = empty_team_store();

    TriggerStore::new(db.clone())
        .create("my-trigger", "message_received", "{}", "ops", "do stuff")
        .unwrap();

    let result = run(
        TriggerAction::Status {
            name: "my-trigger".to_string(),
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
        TriggerAction::Status {
            name: "missing".to_string(),
        },
        db,
        &team_store,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn dispatch_add_with_all_trigger_types_succeeds() {
    for trigger_type in [
        "file_watch",
        "message_received",
        "schedule_complete",
        "webhook_received",
    ] {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("test-team");

        let result = run(
            TriggerAction::Add {
                name: format!("t-{trigger_type}"),
                trigger_type: trigger_type.to_string(),
                team: "test-team".to_string(),
                condition: "{}".to_string(),
                input: "".to_string(),
            },
            db,
            &team_store,
        );
        assert!(
            result.is_ok(),
            "add with type '{trigger_type}' should succeed"
        );
    }
}

#[test]
fn dispatch_add_then_list_then_remove_lifecycle() {
    let db = make_db();
    let (_dir, team_store) = make_team_store_with("ops");

    run(
        TriggerAction::Add {
            name: "lifecycle".to_string(),
            trigger_type: "file_watch".to_string(),
            team: "ops".to_string(),
            condition: "{}".to_string(),
            input: "".to_string(),
        },
        db.clone(),
        &team_store,
    )
    .unwrap();

    run(TriggerAction::List, db.clone(), &team_store).unwrap();

    run(
        TriggerAction::Remove {
            name: "lifecycle".to_string(),
        },
        db.clone(),
        &team_store,
    )
    .unwrap();

    assert!(TriggerStore::new(db).list().unwrap().is_empty());
}
