use super::*;

#[test]
fn load_schedules_page_builds_history_from_matching_runs() {
    with_temp_home(|| {
        save_team("ops");
        let db = test_db();
        ScheduleStore::new(db.clone())
            .create(
                "nightly-ops",
                "0 0 * * * *",
                "ops",
                "",
                Some("2026-03-11 00:00:00"),
            )
            .expect("schedule should seed");
        OrchestrationStore::new(db.clone())
            .create_run(
                "run-1",
                "session-1",
                "ops",
                "chain",
                "Scheduled run: nightly-ops",
                1,
            )
            .expect("run should seed");
        OrchestrationStore::new(db.clone())
            .complete_run("run-1", "done")
            .expect("run should complete");

        let page = load_schedules_page(db, Some("nightly-ops".into())).expect("page should load");

        assert_eq!(page.selected.history.len(), 1);
        assert_eq!(page.selected.history[0].title, "run-1");
        assert_eq!(
            page.selected.history[0].status_label,
            RunStatus::Completed.as_str()
        );
    });
}
