use std::sync::Arc;
use std::time::Duration as StdDuration;

use opengoose_persistence::{Database, OrchestrationStore, TriggerStore};
use opengoose_teams::{OrchestrationPattern, TeamAgent, TeamDefinition, TeamStore};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

pub(crate) use crate::common::{run_async_test, with_temp_home};

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
