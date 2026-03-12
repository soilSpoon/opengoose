use std::sync::Arc;

use opengoose_persistence::{Database, OrchestrationStore, RunStatus, ScheduleStore};
use opengoose_teams::run_due_schedules_once;
use opengoose_types::EventBus;

use crate::support::{run_async_test, seed_team, wait_for_team_run, with_temp_home};

#[test]
fn scheduler_one_shot_advances_due_schedule_and_runs_team() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);

            seed_team("scheduled-team");
            let schedule_store = ScheduleStore::new(db.clone());
            schedule_store
                .create(
                    "nightly-review",
                    "0 0 * * * *",
                    "scheduled-team",
                    "scheduled run input",
                    Some("2000-01-01 00:00:00"),
                )
                .unwrap();

            let before = schedule_store
                .get_by_name("nightly-review")
                .unwrap()
                .unwrap();

            run_due_schedules_once(db.clone(), event_bus.clone())
                .await
                .unwrap();

            let after = schedule_store
                .get_by_name("nightly-review")
                .unwrap()
                .unwrap();
            assert!(after.last_run_at.is_some());
            assert_ne!(after.next_run_at, before.next_run_at);

            assert!(
                wait_for_team_run(&db, "scheduled-team").await,
                "due schedule should create an orchestration run"
            );
            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].team_name, "scheduled-team");
            assert_eq!(runs[0].status, RunStatus::Failed);
        });
    });
}
