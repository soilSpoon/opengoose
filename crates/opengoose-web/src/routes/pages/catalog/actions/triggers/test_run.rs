use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{Database, TriggerStore};
use opengoose_types::EventBus;
use tracing::error;

use crate::data::TriggersPageView;

use super::shared::{danger_notice, selected_page, success_notice};

pub(super) fn test_trigger_page(
    db: Arc<Database>,
    event_bus: EventBus,
    target_name: String,
) -> Result<TriggersPageView> {
    let store = TriggerStore::new(db.clone());

    match store.get_by_name(&target_name)? {
        Some(trigger) => {
            spawn_trigger_test(
                db.clone(),
                event_bus,
                trigger.name.clone(),
                trigger.team_name.clone(),
                resolve_test_input(&trigger.name, &trigger.input),
            );
            selected_page(
                &db,
                Some(target_name.clone()),
                success_notice(format!(
                    "Trigger `{target_name}` test queued. Check Runs for progress."
                )),
            )
        }
        None => selected_page(
            &db,
            None,
            danger_notice(format!("Trigger `{target_name}` was not found.")),
        ),
    }
}

fn spawn_trigger_test(
    db: Arc<Database>,
    event_bus: EventBus,
    trigger_name: String,
    team_name: String,
    run_input: String,
) {
    tokio::spawn(async move {
        match opengoose_teams::run_headless(opengoose_teams::HeadlessConfig::new(
            &team_name,
            &run_input,
            db.clone(),
            event_bus,
        ))
        .await
        {
            Ok(_) => {
                let store = TriggerStore::new(db);
                if let Err(error) = store.mark_fired(&trigger_name) {
                    error!(trigger = %trigger_name, %error, "failed to mark trigger fired after page test");
                }
            }
            Err(error) => {
                error!(trigger = %trigger_name, team = %team_name, %error, "page trigger test failed");
            }
        }
    });
}

fn resolve_test_input(trigger_name: &str, input: &str) -> String {
    let trimmed_input = input.trim();
    if trimmed_input.is_empty() {
        format!("Test run fired from the web dashboard for trigger {trigger_name}")
    } else {
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_test_input;

    #[test]
    fn resolve_test_input_uses_existing_trigger_input() {
        assert_eq!(
            resolve_test_input("nightly", "run the nightly check"),
            "run the nightly check"
        );
    }

    #[test]
    fn resolve_test_input_falls_back_for_blank_trigger_input() {
        assert_eq!(
            resolve_test_input("nightly", "   "),
            "Test run fired from the web dashboard for trigger nightly"
        );
    }
}
