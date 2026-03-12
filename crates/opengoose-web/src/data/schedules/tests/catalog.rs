use super::*;

#[test]
fn load_schedules_page_without_rows_selects_new_draft() {
    with_temp_home(|| {
        save_team("ops");

        let page = load_schedules_page(test_db(), None).expect("page should load");

        assert!(page.schedules.is_empty());
        assert!(page.selected.is_new);
        assert_eq!(page.selected.title, "Create schedule");
        assert_eq!(page.selected.team_options.len(), 1);
    });
}

#[test]
fn load_schedules_page_auto_selects_first_existing_schedule() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        seed_schedule(db.clone(), "alpha");

        let page = load_schedules_page(db, None).expect("page should load");

        assert!(!page.selected.is_new);
        assert_eq!(page.selected.name, "alpha");
    });
}

#[test]
fn load_schedules_page_selects_new_draft_when_new_key_passed() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        seed_schedule(db.clone(), "existing");

        let page =
            load_schedules_page(db, Some(NEW_SCHEDULE_KEY.into())).expect("page should load");

        assert!(page.selected.is_new);
        assert_eq!(page.selected.title, "Create schedule");
    });
}

#[test]
fn mode_label_reflects_enabled_and_total_counts() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        seed_schedule(db.clone(), "active-one");

        save_schedule(
            db.clone(),
            ScheduleSaveInput {
                cron_expression: "0 6 * * * *".into(),
                ..new_schedule_input("paused-one")
            },
        )
        .expect("second schedule should save");

        toggle_schedule(db.clone(), "paused-one".into()).expect("toggle should succeed");

        let page = load_schedules_page(db, None).expect("page should load");

        assert_eq!(page.mode_label, "1 active of 2");
        assert_eq!(page.mode_tone, "success");
    });
}

#[test]
fn mode_tone_is_amber_when_all_schedules_paused() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        seed_schedule(db.clone(), "paused");

        toggle_schedule(db.clone(), "paused".into()).expect("toggle should succeed");

        let page = load_schedules_page(db, None).expect("page should load");

        assert_eq!(page.mode_label, "0 active of 1");
        assert_eq!(page.mode_tone, "amber");
    });
}
