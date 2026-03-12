use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use opengoose_persistence::{Database, OrchestrationStore, TriggerStore};
use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore, CommunicationMode};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn with_temp_home(test: impl FnOnce()) {
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

pub(crate) fn run_async_test(test: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(test);
}

pub(crate) fn seed_team(name: &str) {
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
                goal: None,
                communication_mode: CommunicationMode::default(),
            },
            false,
        )
        .unwrap();
}

pub(crate) fn seed_trigger(
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

pub(crate) async fn wait_for_trigger_fire_count(
    db: &Arc<Database>,
    name: &str,
    expected: i32,
) -> bool {
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

pub(crate) async fn trigger_fired_within(
    db: &Arc<Database>,
    name: &str,
    timeout_duration: StdDuration,
) -> bool {
    timeout(timeout_duration, async {
        loop {
            let trigger = TriggerStore::new(db.clone())
                .get_by_name(name)
                .unwrap()
                .unwrap();
            if trigger.fire_count > 0 {
                return;
            }
            sleep(StdDuration::from_millis(25)).await;
        }
    })
    .await
    .is_ok()
}

pub(crate) async fn wait_for_team_run(db: &Arc<Database>, team_name: &str) -> bool {
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

pub(crate) async fn stop_watcher(handle: tokio::task::JoinHandle<()>, cancel: CancellationToken) {
    cancel.cancel();
    let _ = timeout(StdDuration::from_secs(1), handle).await;
}
