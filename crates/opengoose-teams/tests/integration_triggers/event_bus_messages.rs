use std::sync::Arc;
use std::time::Duration as StdDuration;

use opengoose_persistence::{Database, OrchestrationStore, RunStatus, TriggerStore};
use opengoose_teams::spawn_event_bus_trigger_watcher;
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};
use tokio_util::sync::CancellationToken;

use crate::support::{
    run_async_test, seed_team, seed_trigger, stop_watcher, trigger_fired_within, wait_for_team_run,
    wait_for_trigger_fire_count, with_temp_home,
};

#[test]
fn event_bus_trigger_watcher_fires_on_message_events() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("event-message-team");
            seed_trigger(
                &db,
                "on-message-trigger",
                "on_message",
                r#"{"from_author":"alice","content_contains":"hello"}"#,
                "event-message-team",
                "run on_message",
            );

            let handle =
                spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

            // Yield to let the spawned watcher task subscribe to the event bus.
            tokio::task::yield_now().await;

            event_bus.emit(AppEventKind::MessageReceived {
                session_key: SessionKey::new(Platform::Discord, "guild", "channel"),
                author: "alice".into(),
                content: "say hello".into(),
            });

            assert!(
                wait_for_trigger_fire_count(&db, "on-message-trigger", 1).await,
                "on_message trigger should fire"
            );
            assert!(
                wait_for_team_run(&db, "event-message-team").await,
                "on_message trigger should create an orchestration run"
            );
            let trigger = TriggerStore::new(db.clone())
                .get_by_name("on-message-trigger")
                .unwrap()
                .unwrap();
            assert_eq!(trigger.fire_count, 1);

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].team_name, "event-message-team");
            assert_eq!(runs[0].status, RunStatus::Failed);

            stop_watcher(handle, cancel).await;
        });
    });
}

#[test]
fn event_bus_trigger_watcher_fires_on_schedule_events() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("event-schedule-team");
            seed_trigger(
                &db,
                "on-schedule-trigger",
                "on_schedule",
                r#"{"team":"event-schedule-team"}"#,
                "event-schedule-team",
                "run on schedule",
            );

            let handle =
                spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

            // Yield to let the spawned watcher task subscribe to the event bus.
            tokio::task::yield_now().await;

            event_bus.emit(AppEventKind::TeamRunCompleted {
                team: "event-schedule-team".into(),
            });

            assert!(
                wait_for_trigger_fire_count(&db, "on-schedule-trigger", 1).await,
                "on_schedule trigger should fire"
            );
            assert!(
                wait_for_team_run(&db, "event-schedule-team").await,
                "on_schedule trigger should create an orchestration run"
            );
            let trigger = TriggerStore::new(db.clone())
                .get_by_name("on-schedule-trigger")
                .unwrap()
                .unwrap();
            assert_eq!(trigger.fire_count, 1);

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].team_name, "event-schedule-team");
            assert_eq!(runs[0].status, RunStatus::Failed);

            stop_watcher(handle, cancel).await;
        });
    });
}

#[test]
fn multiple_triggers_fire_for_same_event() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("alpha-team");
            seed_team("beta-team");
            seed_trigger(
                &db,
                "alpha-trigger",
                "on_message",
                r#"{}"#,
                "alpha-team",
                "run alpha",
            );
            seed_trigger(
                &db,
                "beta-trigger",
                "on_message",
                r#"{}"#,
                "beta-team",
                "run beta",
            );

            let handle =
                spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

            tokio::task::yield_now().await;

            event_bus.emit(AppEventKind::MessageReceived {
                session_key: SessionKey::new(Platform::Slack, "workspace", "channel"),
                author: "user".into(),
                content: "trigger both".into(),
            });

            assert!(
                wait_for_trigger_fire_count(&db, "alpha-trigger", 1).await,
                "alpha trigger should fire"
            );
            assert!(
                wait_for_trigger_fire_count(&db, "beta-trigger", 1).await,
                "beta trigger should fire"
            );

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 2, "both teams should have orchestration runs");

            stop_watcher(handle, cancel).await;
        });
    });
}

#[test]
fn disabled_trigger_does_not_fire() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("disabled-team");
            seed_trigger(
                &db,
                "disabled-trigger",
                "on_message",
                r#"{}"#,
                "disabled-team",
                "should not run",
            );

            TriggerStore::new(db.clone())
                .set_enabled("disabled-trigger", false)
                .unwrap();

            let handle =
                spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

            tokio::task::yield_now().await;

            event_bus.emit(AppEventKind::MessageReceived {
                session_key: SessionKey::new(Platform::Discord, "guild", "channel"),
                author: "anyone".into(),
                content: "hello".into(),
            });

            assert!(
                !trigger_fired_within(&db, "disabled-trigger", StdDuration::from_millis(400)).await,
                "disabled trigger must not fire"
            );

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 0, "no run created for a disabled trigger");

            stop_watcher(handle, cancel).await;
        });
    });
}
