use super::*;

#[test]
fn toggle_schedule_flips_enabled_state() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        seed_schedule(db.clone(), "nightly-ops");

        let page = toggle_schedule(db, "nightly-ops".into()).expect("toggle should succeed");

        assert!(!page.selected.enabled);
        assert_eq!(
            page.selected
                .notice
                .as_ref()
                .map(|notice| notice.text.as_str()),
            Some("Schedule paused.")
        );
    });
}

#[test]
fn toggle_schedule_enables_paused_schedule() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        seed_schedule(db.clone(), "paused-schedule");

        toggle_schedule(db.clone(), "paused-schedule".into()).expect("first toggle should succeed");

        let page =
            toggle_schedule(db, "paused-schedule".into()).expect("second toggle should succeed");

        assert!(page.selected.enabled);
        assert_eq!(
            page.selected.notice.as_ref().map(|n| n.text.as_str()),
            Some("Schedule enabled.")
        );
    });
}

#[test]
fn toggle_schedule_returns_danger_notice_for_missing_schedule() {
    with_temp_home(|| {
        save_team("ops");

        let page =
            toggle_schedule(test_db(), "nonexistent".into()).expect("should render error page");

        assert_eq!(
            page.selected.notice.as_ref().map(|n| n.tone),
            Some("danger")
        );
        assert!(page
            .selected
            .notice
            .as_ref()
            .map(|n| n.text.contains("nonexistent"))
            .unwrap_or(false));
    });
}

#[test]
fn delete_schedule_requires_confirmation() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        seed_schedule(db.clone(), "nightly-ops");

        let page = delete_schedule(db, "nightly-ops".into(), false).expect("delete should render");

        assert_eq!(page.schedules.len(), 1);
        assert_eq!(
            page.selected.notice.as_ref().map(|notice| notice.tone),
            Some("danger")
        );
    });
}

#[test]
fn delete_schedule_removes_with_confirmation() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        seed_schedule(db.clone(), "to-delete");

        let page = delete_schedule(db, "to-delete".into(), true).expect("delete should succeed");

        assert!(page.schedules.is_empty());
        assert_eq!(
            page.selected.notice.as_ref().map(|n| n.tone),
            Some("success")
        );
        assert!(page
            .selected
            .notice
            .as_ref()
            .map(|n| n.text.contains("to-delete"))
            .unwrap_or(false));
    });
}

#[test]
fn delete_schedule_handles_already_removed_schedule() {
    with_temp_home(|| {
        save_team("ops");

        let page = delete_schedule(test_db(), "ghost".into(), true).expect("delete should render");

        assert_eq!(
            page.selected.notice.as_ref().map(|n| n.tone),
            Some("danger")
        );
        assert!(page
            .selected
            .notice
            .as_ref()
            .map(|n| n.text.contains("ghost"))
            .unwrap_or(false));
    });
}
