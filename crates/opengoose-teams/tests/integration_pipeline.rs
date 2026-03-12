use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use opengoose_persistence::{
    AgentMessageStore, Database, OrchestrationStore, RunStatus, TriggerStore,
};
use opengoose_teams::{
    HeadlessConfig, TeamDefinition, TeamStore, message_bus::MessageBus, run_headless, spawn_trigger_watcher,
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

fn pipeline_team_yaml(name: &str, profile: &str) -> String {
    format!(
        "version: \"1.0.0\"\ntitle: {name}\ndescription: Integration pipeline team\nworkflow: chain\nagents:\n  - profile: {profile}\n    role: \"integration role\"\n"
    )
}

fn seed_team(name: &str, profile: &str) {
    let store = TeamStore::new().unwrap();
    let team = TeamDefinition::from_yaml(&pipeline_team_yaml(name, profile)).unwrap();
    store.save(&team, false).unwrap();
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

async fn wait_for_app_event_match<F>(
    rx: &mut tokio::sync::broadcast::Receiver<opengoose_types::AppEvent>,
    predicate: F,
) -> bool
where
    F: FnMut(&opengoose_types::AppEventKind) -> bool,
{
    let mut predicate = predicate;
    timeout(StdDuration::from_secs(2), async {
        loop {
            if let Ok(event) = rx.recv().await
                && predicate(&event.kind)
            {
                return;
            }
        }
    })
    .await
    .is_ok()
}

#[test]
fn team_yaml_from_cli_parse_is_saved_for_pipeline_reuse() {
    with_temp_home(|| {
        let yaml = pipeline_team_yaml("pipeline-team", "missing-profile");
        let team = TeamDefinition::from_yaml(&yaml).unwrap();
        assert_eq!(team.name(), "pipeline-team");
        assert_eq!(team.agents.len(), 1);

        let store = TeamStore::new().unwrap();
        store.save(&team, false).unwrap();

        let stored = store.get("pipeline-team").unwrap();
        assert_eq!(stored.agents[0].profile, "missing-profile");
    });
}

#[test]
fn team_yaml_parser_reports_router_config_error() {
    with_temp_home(|| {
        let yaml = r#"version: "1.0.0"
title: invalid-router
workflow: router
agents:
  - profile: worker
  - profile: reviewer
"#;

        let err = TeamDefinition::from_yaml(yaml).unwrap_err();
        assert!(
            err.to_string()
                .contains("router workflow requires a `router` configuration")
        );
    });
}

#[test]
fn team_store_rejects_duplicate_save_without_force() {
    with_temp_home(|| {
        let store = TeamStore::new().unwrap();
        let team =
            TeamDefinition::from_yaml(&pipeline_team_yaml("duplicate-team", "missing-profile"))
                .unwrap();
        store.save(&team, false).unwrap();

        let err = store.save(&team, false).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    });
}

#[test]
fn message_bus_trigger_starts_workflow_and_persists_failed_status() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);
            let message_bus = MessageBus::new(16);
            let cancel = CancellationToken::new();

            seed_team("pipeline-run-team", "missing-profile");
            seed_trigger(
                &db,
                "message-trigger-pipeline",
                "message_received",
                r#"{}"#,
                "pipeline-run-team",
                "integration run",
            );

            let mut processor_rx = message_bus.subscribe_agent("processor");
            let mut event_rx = event_bus.subscribe();
            let handle = spawn_trigger_watcher(
                db.clone(),
                event_bus.clone(),
                message_bus.clone(),
                cancel.clone(),
            );

            // Ensure the trigger watcher and directed subscriber are ready.
            tokio::task::yield_now().await;

            let delivered = message_bus.send_directed("orchestrator", "processor", "run this team");
            assert_eq!(delivered, 1);

            let event = timeout(StdDuration::from_millis(250), processor_rx.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(event.from, "orchestrator");
            assert_eq!(event.to.as_deref(), Some("processor"));
            assert_eq!(event.payload, "run this team");

            assert!(
                wait_for_trigger_fire_count(&db, "message-trigger-pipeline", 1).await,
                "team trigger should fire"
            );
            assert!(
                wait_for_team_run(&db, "pipeline-run-team").await,
                "workflow run should be created"
            );

            let mut runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert_eq!(runs.len(), 1);
            let run = runs.remove(0);
            assert_eq!(run.team_name, "pipeline-run-team");
            assert_eq!(run.status, RunStatus::Failed);
            assert_eq!(run.workflow, "chain");
            assert!(
                run.result
                    .as_deref()
                    .is_some_and(|r| r.contains("profile `missing-profile` not found"))
            );

            let mut saw_started = false;
            let mut saw_failed = false;
            let run_events_seen = wait_for_app_event_match(&mut event_rx, |event| {
                match event {
                    AppEventKind::TeamRunStarted { team, workflow, .. }
                        if team == "pipeline-run-team" && workflow == "chain" =>
                    {
                        saw_started = true;
                    }
                    AppEventKind::TeamRunFailed { team, reason }
                        if team == "pipeline-run-team"
                            && reason.contains("profile `missing-profile`") =>
                    {
                        saw_failed = true;
                    }
                    _ => {}
                }
                saw_started && saw_failed
            })
            .await;
            assert!(
                run_events_seen,
                "workflow should emit start and failure events on the event bus"
            );

            stop_watcher(handle, cancel).await;
        });
    });
}

#[test]
fn message_bus_message_is_persisted_for_receiver() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let message_bus = MessageBus::new(8);
            let mut receiver = message_bus.subscribe_agent("collector");
            let store = AgentMessageStore::new(db.clone());
            let session_key = SessionKey::new(Platform::Custom("pipeline".into()), "suite", "bus");

            let sent_to_directed = message_bus.send_directed("sensor", "collector", "payload-1");
            assert_eq!(sent_to_directed, 1);

            let payload = timeout(StdDuration::from_millis(250), async {
                receiver.recv().await
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(payload.from, "sensor");
            assert_eq!(payload.payload, "payload-1");

            let _ = store
                .send_directed(
                    &session_key.to_stable_id(),
                    "sensor",
                    "collector",
                    "payload-1",
                )
                .unwrap();
            let pending = store
                .receive_pending(&session_key.to_stable_id(), "collector")
                .unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].payload, "payload-1");
        });
    });
}

#[test]
fn run_headless_reports_missing_team_error_before_persistence() {
    with_temp_home(|| {
        run_async_test(async {
            let db = Arc::new(Database::open_in_memory().unwrap());
            let event_bus = EventBus::new(16);

            let err = run_headless(HeadlessConfig::new("missing-team", "integration input", db.clone(), event_bus))
                .await
                .unwrap_err();
            assert!(err.to_string().contains("missing-team"));

            let runs = OrchestrationStore::new(db.clone())
                .list_runs(None, 10)
                .unwrap();
            assert!(runs.is_empty());
        });
    });
}
