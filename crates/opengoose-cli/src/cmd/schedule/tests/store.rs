use super::support::make_store;

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
