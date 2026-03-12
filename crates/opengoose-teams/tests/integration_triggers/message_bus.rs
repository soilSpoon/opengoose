use std::sync::Arc;
use std::time::Duration as StdDuration;

use opengoose_persistence::{Database, OrchestrationStore, RunStatus, TriggerStore};
use opengoose_teams::{message_bus::MessageBus, spawn_trigger_watcher};
use opengoose_types::EventBus;
use tokio_util::sync::CancellationToken;

use crate::support::{
    run_async_test, seed_team, seed_trigger, stop_watcher, trigger_fired_within, wait_for_team_run,
    wait_for_trigger_fire_count, with_temp_home,
};

#[test]
fn message_bus_trigger_watcher_fires_and_increments_fire_count() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let message_bus = MessageBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("message-team");
            seed_trigger(
                &db,
                "message-received-trigger",
                "message_received",
                r#"{"from_agent":"agent-a"}"#,
                "message-team",
                "run on message event",
            );

            let handle = spawn_trigger_watcher(
                db.clone(),
                event_bus.clone(),
                message_bus.clone(),
                cancel.clone(),
            );

            // Yield to let the spawned watcher task subscribe to the message bus.
            tokio::task::yield_now().await;

            message_bus.send_directed("agent-a", "agent-b", "hello from agent-a");

            assert!(
                wait_for_trigger_fire_count(&db, "message-received-trigger", 1).await,
                "message trigger should fire at least once"
            );

            let trigger = TriggerStore::new(db.clone())
                .get_by_name("message-received-trigger")
                .unwrap()
                .unwrap();
            assert_eq!(trigger.fire_count, 1);

            assert!(
                wait_for_team_run(&db, "message-team").await,
                "message trigger should create an orchestration run"
            );
            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].team_name, "message-team");
            assert_eq!(runs[0].status, RunStatus::Failed);

            stop_watcher(handle, cancel).await;
        });
    });
}

#[test]
fn message_bus_trigger_with_channel_filter_fires_only_on_matching_channel() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let message_bus = MessageBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("alerts-team");
            seed_trigger(
                &db,
                "channel-filter-trigger",
                "message_received",
                r#"{"channel":"alerts"}"#,
                "alerts-team",
                "run on alerts channel",
            );

            let handle = spawn_trigger_watcher(
                db.clone(),
                event_bus.clone(),
                message_bus.clone(),
                cancel.clone(),
            );

            tokio::task::yield_now().await;

            // Publish to a non-matching channel first.
            message_bus.publish("agent-x", "general", "routine message");

            assert!(
                !trigger_fired_within(
                    &db,
                    "channel-filter-trigger",
                    StdDuration::from_millis(150),
                )
                .await,
                "should not fire on wrong channel"
            );

            message_bus.publish("agent-x", "alerts", "critical alert");

            assert!(
                wait_for_trigger_fire_count(&db, "channel-filter-trigger", 1).await,
                "trigger should fire on matching channel"
            );

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].team_name, "alerts-team");

            stop_watcher(handle, cancel).await;
        });
    });
}
