use opengoose_persistence::Trigger;
use tracing::{error, info};

use super::payload::MatchedWebhookTrigger;
use crate::state::AppState;

pub(super) fn dispatch_matching_triggers(
    state: &AppState,
    normalized_path: &str,
    matching: Vec<MatchedWebhookTrigger>,
) -> String {
    let fired_name = matching
        .first()
        .map(|matched| matched.trigger.name.clone())
        .expect("matched webhook trigger list should not be empty");

    for matched in matching {
        spawn_trigger_run(state, normalized_path, matched.trigger);
    }

    fired_name
}

fn spawn_trigger_run(state: &AppState, normalized_path: &str, trigger: Trigger) {
    let db = state.db.clone();
    let event_bus = state.event_bus.clone();
    let trigger_store = state.trigger_store.clone();
    let team_name = trigger.team_name.clone();
    let trigger_name = trigger.name.clone();
    let trigger_input = trigger_input(&trigger, normalized_path);

    tokio::spawn(async move {
        info!(
            trigger = %trigger_name,
            team = %team_name,
            "firing webhook-received trigger"
        );
        match opengoose_teams::run_headless(&team_name, &trigger_input, db, event_bus).await {
            Ok((run_id, _)) => {
                info!(trigger = %trigger_name, run_id, "webhook-triggered team run completed");
            }
            Err(error) => {
                error!(
                    trigger = %trigger_name,
                    team = %team_name,
                    %error,
                    "webhook-triggered team run failed"
                );
            }
        }
        if let Err(error) = trigger_store.mark_fired(&trigger_name) {
            error!(trigger = %trigger_name, %error, "failed to mark webhook trigger as fired");
        }
    });
}

fn trigger_input(trigger: &Trigger, normalized_path: &str) -> String {
    if trigger.input.is_empty() {
        format!("Triggered by incoming webhook at {normalized_path}")
    } else {
        trigger.input.clone()
    }
}
