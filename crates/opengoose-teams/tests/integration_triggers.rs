use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use opengoose_persistence::{Database, OrchestrationStore, RunStatus, ScheduleStore, TriggerStore};
use opengoose_teams::{
    OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore, message_bus::MessageBus,
    run_due_schedules_once, spawn_event_bus_trigger_watcher, spawn_trigger_watcher,
};
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home(test: impl FnOnce()) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let temp_home =
        std::env::temp_dir().join(format!("opengoose-integration-home-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_home).unwrap();

    let saved_home = std::env::var("HOME").ok();
    unsafe {
        std::env::set_var("HOME", &temp_home);
    }

    test();

    unsafe {
        match saved_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
    let _ = std::fs::remove_dir_all(&temp_home);
}

fn run_async_test(test: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(test);
}

fn seed_team(name: &str) {
    let store = TeamStore::new().unwrap();
    store
        .save(
            &TeamDefinition {
                version: "1.0.0".into(),
                title: name.into(),
                description: Some("integration test team".into()),
                workflow: OrchestrationPattern::Chain,
                agents: vec![TeamAgent {
                    profile: "missing-profile".into(),
                    role: Some("no-op".into()),
                }],
                router: None,
                fan_out: None,
            },
            false,
        )
        .unwrap();
}

fn seed_trigger(
    db: &Arc<Database>,
    name: &str,
    trigger_type: &str,
    condition_json: &str,
    team_name: &str,
    input: &str,
) {
    TriggerStore::new(db.clone())
        .create(name, trigger_type, condition_json, team_name, input)
        .unwrap();
}

async fn wait_for_trigger_fire_count(db: &Arc<Database>, name: &str, expected: i32) -> bool {
    timeout(StdDuration::from_secs(2), async {
        loop {
            let trigger = TriggerStore::new(db.clone()).get_by_name(name).unwrap();
            if let Some(trigger) = trigger
                && trigger.fire_count >= expected
            {
                return;
            }
            sleep(StdDuration::from_millis(25)).await;
        }
    })
    .await
    .is_ok()
}

async fn wait_for_team_run(db: &Arc<Database>, team_name: &str) -> bool {
    timeout(StdDuration::from_secs(2), async {
        loop {
            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            if runs.iter().any(|run| run.team_name == team_name) {
                return;
            }
            sleep(StdDuration::from_millis(25)).await;
        }
    })
    .await
    .is_ok()
}

async fn stop_watcher(handle: tokio::task::JoinHandle<()>, cancel: CancellationToken) {
    cancel.cancel();
    let _ = timeout(StdDuration::from_secs(1), handle).await;
}

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

            // Yield to let the spawned watcher task subscribe to the message bus
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

            // Yield to let the spawned watcher task subscribe to the event bus
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

            // Yield to let the spawned watcher task subscribe to the event bus
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

            let did_fire = timeout(StdDuration::from_millis(400), async {
                loop {
                    let trigger = TriggerStore::new(db.clone())
                        .get_by_name("on-session-start-miss-trigger")
                        .unwrap()
                        .unwrap();
                    if trigger.fire_count > 0 {
                        break;
                    }
                    sleep(StdDuration::from_millis(25)).await;
                }
            })
            .await
            .is_ok();

            assert!(
                !did_fire,
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

            let trigger = opengoose_persistence::TriggerStore::new(db.clone())
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
            // No platform filter — matches the synthetic "system" platform from GooseReady.
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

#[test]
fn multiple_triggers_fire_for_same_event() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("alpha-team");
            seed_team("beta-team");
            // Two wildcard on_message triggers — both must fire for the same event.
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

            opengoose_persistence::TriggerStore::new(db.clone())
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

            let did_fire = tokio::time::timeout(std::time::Duration::from_millis(400), async {
                loop {
                    let t = opengoose_persistence::TriggerStore::new(db.clone())
                        .get_by_name("disabled-trigger")
                        .unwrap()
                        .unwrap();
                    if t.fire_count > 0 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
            })
            .await
            .is_ok();

            assert!(!did_fire, "disabled trigger must not fire");

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 0, "no run created for a disabled trigger");

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

            // Publish to a non-matching channel first — trigger must NOT fire.
            message_bus.publish("agent-x", "general", "routine message");

            // Brief wait to confirm no fire.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;

            let trigger = opengoose_persistence::TriggerStore::new(db.clone())
                .get_by_name("channel-filter-trigger")
                .unwrap()
                .unwrap();
            assert_eq!(trigger.fire_count, 0, "should not fire on wrong channel");

            // Now publish to the matching channel.
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
