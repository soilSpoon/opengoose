use std::sync::Arc;
use std::time::Duration as StdDuration;

use opengoose_persistence::{Database, OrchestrationStore, RunStatus, TriggerStore};
use opengoose_teams::spawn_event_bus_trigger_watcher;
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};
use tokio_util::sync::CancellationToken;

use crate::support::{
    run_async_test, seed_team, seed_trigger, stop_watcher, trigger_fired_within,
    wait_for_trigger_fire_count, with_temp_home,
};

#[test]
fn event_bus_trigger_watcher_fires_on_session_start_events() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("session-start-team");
            seed_trigger(
                &db,
                "on-session-start-trigger",
                "on_session_start",
                r#"{"platform":"discord"}"#,
                "session-start-team",
                "run on session ready",
            );

            let handle =
                spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

            // Yield so the watcher subscribes before the event is emitted.
            tokio::task::yield_now().await;

            event_bus.emit(AppEventKind::ChannelReady {
                platform: Platform::Discord,
            });

            assert!(
                wait_for_trigger_fire_count(&db, "on-session-start-trigger", 1).await,
                "on_session_start trigger should fire for matching platform"
            );

            let trigger = TriggerStore::new(db.clone())
                .get_by_name("on-session-start-trigger")
                .unwrap()
                .unwrap();
            assert_eq!(trigger.fire_count, 1);

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].team_name, "session-start-team");
            assert_eq!(runs[0].status, RunStatus::Failed);

            stop_watcher(handle, cancel).await;
        });
    });
}

#[test]
fn event_bus_trigger_watcher_does_not_fire_when_session_start_mismatched() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("session-start-miss-team");
            seed_trigger(
                &db,
                "on-session-start-miss-trigger",
                "on_session_start",
                r#"{"platform":"slack"}"#,
                "session-start-miss-team",
                "run only on slack session",
            );

            let handle =
                spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

            // Yield so the watcher subscribes before the event is emitted.
            tokio::task::yield_now().await;

            event_bus.emit(AppEventKind::ChannelReady {
                platform: Platform::Discord,
            });

            assert!(
                !trigger_fired_within(
                    &db,
                    "on-session-start-miss-trigger",
                    StdDuration::from_millis(400),
                )
                .await,
                "on_session_start trigger should not fire for mismatched platform"
            );

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 0);

            stop_watcher(handle, cancel).await;
        });
    });
}

#[test]
fn event_bus_trigger_watcher_fires_on_session_end() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("session-end-team");
            seed_trigger(
                &db,
                "on-session-end-trigger",
                "on_session_end",
                r#"{"platform":"slack"}"#,
                "session-end-team",
                "run on session disconnect",
            );

            let handle =
                spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

            tokio::task::yield_now().await;

            event_bus.emit(AppEventKind::SessionDisconnected {
                session_key: SessionKey::new(Platform::Slack, "workspace", "channel"),
                reason: "user left".into(),
            });

            assert!(
                wait_for_trigger_fire_count(&db, "on-session-end-trigger", 1).await,
                "on_session_end trigger should fire on SessionDisconnected"
            );

            let trigger = TriggerStore::new(db.clone())
                .get_by_name("on-session-end-trigger")
                .unwrap()
                .unwrap();
            assert_eq!(trigger.fire_count, 1);

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].team_name, "session-end-team");

            stop_watcher(handle, cancel).await;
        });
    });
}

#[test]
fn event_bus_trigger_watcher_fires_on_goose_ready() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("goose-ready-team");
            seed_trigger(
                &db,
                "on-goose-ready-trigger",
                "on_session_start",
                r#"{}"#,
                "goose-ready-team",
                "run on goose ready",
            );

            let handle =
                spawn_event_bus_trigger_watcher(db.clone(), event_bus.clone(), cancel.clone());

            tokio::task::yield_now().await;

            event_bus.emit(AppEventKind::GooseReady);

            assert!(
                wait_for_trigger_fire_count(&db, "on-goose-ready-trigger", 1).await,
                "on_session_start trigger should fire on GooseReady"
            );

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].team_name, "goose-ready-team");

            stop_watcher(handle, cancel).await;
        });
    });
}
