use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_persistence::{Database, TriggerStore};
use opengoose_teams::triggers;

#[derive(Subcommand)]
/// Subcommands for `opengoose trigger`.
pub enum TriggerAction {
    /// Add a new event trigger
    Add {
        /// Unique name for this trigger
        name: String,
        /// Trigger type (file_watch, message_received, schedule_complete, webhook_received)
        #[arg(long, name = "type")]
        trigger_type: String,
        /// Team name to run when the trigger fires
        #[arg(long)]
        team: String,
        /// JSON condition for matching (e.g. '{"channel":"alerts"}')
        #[arg(long, default_value = "{}")]
        condition: String,
        /// Input text for the team (optional)
        #[arg(long, default_value = "")]
        input: String,
    },
    /// List all triggers
    List,
    /// Remove a trigger
    Remove {
        /// Trigger name
        name: String,
    },
    /// Enable a trigger
    Enable {
        /// Trigger name
        name: String,
    },
    /// Disable a trigger
    Disable {
        /// Trigger name
        name: String,
    },
    /// Show status of a specific trigger
    Status {
        /// Trigger name
        name: String,
    },
}

/// Dispatch and execute the selected trigger subcommand.
pub fn execute(action: TriggerAction) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let team_store = opengoose_teams::TeamStore::new()?;
    run(action, db, &team_store)
}

/// Testable dispatch: accepts injected db and team_store.
pub(crate) fn run(
    action: TriggerAction,
    db: Arc<Database>,
    team_store: &opengoose_teams::TeamStore,
) -> Result<()> {
    let store = TriggerStore::new(db);
    match action {
        TriggerAction::Add {
            name,
            trigger_type,
            team,
            condition,
            input,
        } => cmd_add(
            &store,
            team_store,
            &name,
            &trigger_type,
            &team,
            &condition,
            &input,
        ),
        TriggerAction::List => cmd_list(&store),
        TriggerAction::Remove { name } => cmd_remove(&store, &name),
        TriggerAction::Enable { name } => cmd_enable(&store, &name),
        TriggerAction::Disable { name } => cmd_disable(&store, &name),
        TriggerAction::Status { name } => cmd_status(&store, &name),
    }
}

fn cmd_add(
    store: &TriggerStore,
    team_store: &opengoose_teams::TeamStore,
    name: &str,
    trigger_type: &str,
    team: &str,
    condition: &str,
    input: &str,
) -> Result<()> {
    // Validate trigger type
    triggers::validate_trigger_type(trigger_type).map_err(|e| anyhow::anyhow!(e))?;

    // Validate condition JSON
    serde_json::from_str::<serde_json::Value>(condition)
        .map_err(|e| anyhow::anyhow!("invalid condition JSON: {e}"))?;

    // Verify team exists
    if team_store.get(team).is_err() {
        bail!(
            "team '{}' not found. Use `opengoose team list` to see available teams.",
            team
        );
    }

    let trigger = store.create(name, trigger_type, condition, team, input)?;

    println!("Created trigger '{}'.", trigger.name);
    println!("  Type: {}", trigger.trigger_type);
    println!("  Team: {}", trigger.team_name);
    if trigger.condition_json != "{}" {
        println!("  Condition: {}", trigger.condition_json);
    }

    Ok(())
}

fn cmd_list(store: &TriggerStore) -> Result<()> {
    let triggers = store.list()?;

    if triggers.is_empty() {
        println!("No triggers found. Use `opengoose trigger add` to create one.");
        return Ok(());
    }

    println!(
        "{:<20} {:<10} {:<20} {:<20} {:<10}",
        "NAME", "ENABLED", "TYPE", "TEAM", "FIRED"
    );
    for t in &triggers {
        let enabled = if t.enabled { "yes" } else { "no" };
        println!(
            "{:<20} {:<10} {:<20} {:<20} {:<10}",
            t.name, enabled, t.trigger_type, t.team_name, t.fire_count
        );
    }

    Ok(())
}

fn cmd_remove(store: &TriggerStore, name: &str) -> Result<()> {
    if store.remove(name)? {
        println!("Removed trigger '{name}'.");
    } else {
        bail!("trigger '{name}' not found.");
    }

    Ok(())
}

fn cmd_enable(store: &TriggerStore, name: &str) -> Result<()> {
    if store.set_enabled(name, true)? {
        println!("Enabled trigger '{name}'.");
    } else {
        bail!("trigger '{name}' not found.");
    }

    Ok(())
}

fn cmd_disable(store: &TriggerStore, name: &str) -> Result<()> {
    if store.set_enabled(name, false)? {
        println!("Disabled trigger '{name}'.");
    } else {
        bail!("trigger '{name}' not found.");
    }

    Ok(())
}

fn cmd_status(store: &TriggerStore, name: &str) -> Result<()> {
    let trigger = store
        .get_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("trigger '{}' not found", name))?;

    println!("Trigger: {}", trigger.name);
    println!("  Type: {}", trigger.trigger_type);
    println!("  Team: {}", trigger.team_name);
    println!("  Enabled: {}", if trigger.enabled { "yes" } else { "no" });
    println!("  Fire count: {}", trigger.fire_count);
    println!(
        "  Last fired: {}",
        trigger.last_fired_at.as_deref().unwrap_or("never")
    );
    if trigger.condition_json != "{}" {
        println!("  Condition: {}", trigger.condition_json);
    }
    if !trigger.input.is_empty() {
        let preview = if trigger.input.len() > 100 {
            format!(
                "{}...",
                &trigger.input[..trigger.input.floor_char_boundary(100)]
            )
        } else {
            trigger.input.clone()
        };
        println!("  Input: {preview}");
    }
    println!("  Created: {}", trigger.created_at);
    println!("  Updated: {}", trigger.updated_at);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> TriggerStore {
        let db = Arc::new(opengoose_persistence::Database::open_in_memory().unwrap());
        TriggerStore::new(db)
    }

    fn make_db() -> Arc<Database> {
        Arc::new(opengoose_persistence::Database::open_in_memory().unwrap())
    }

    /// Create a temporary TeamStore with a named team YAML file.
    fn make_team_store_with(team_name: &str) -> (tempfile::TempDir, opengoose_teams::TeamStore) {
        let dir = tempfile::tempdir().unwrap();
        let yaml = format!(
            "version: \"1.0\"\ntitle: {team_name}\nworkflow: chain\nagents:\n  - profile: default\n"
        );
        std::fs::write(dir.path().join(format!("{team_name}.yaml")), yaml).unwrap();
        let store = opengoose_teams::TeamStore::with_dir(dir.path().to_path_buf());
        (dir, store)
    }

    fn empty_team_store() -> (tempfile::TempDir, opengoose_teams::TeamStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = opengoose_teams::TeamStore::with_dir(dir.path().to_path_buf());
        (dir, store)
    }

    // ---- validate_trigger_type ----

    #[test]
    fn validate_trigger_type_accepts_file_watch() {
        assert!(triggers::validate_trigger_type("file_watch").is_ok());
    }

    #[test]
    fn validate_trigger_type_accepts_message_received() {
        assert!(triggers::validate_trigger_type("message_received").is_ok());
    }

    #[test]
    fn validate_trigger_type_accepts_schedule_complete() {
        assert!(triggers::validate_trigger_type("schedule_complete").is_ok());
    }

    #[test]
    fn validate_trigger_type_accepts_webhook_received() {
        assert!(triggers::validate_trigger_type("webhook_received").is_ok());
    }

    #[test]
    fn validate_trigger_type_rejects_invalid() {
        let err = triggers::validate_trigger_type("kafka_event").unwrap_err();
        assert!(err.contains("kafka_event"));
    }

    #[test]
    fn validate_trigger_type_rejects_empty_string() {
        assert!(triggers::validate_trigger_type("").is_err());
    }

    // ---- JSON condition validation (mirrors cmd_add logic) ----

    #[test]
    fn condition_json_valid_object() {
        let result = serde_json::from_str::<serde_json::Value>(r#"{"channel":"alerts"}"#);
        assert!(result.is_ok());
    }

    #[test]
    fn condition_json_empty_object_is_valid() {
        let result = serde_json::from_str::<serde_json::Value>("{}");
        assert!(result.is_ok());
    }

    #[test]
    fn condition_json_invalid_returns_error() {
        let result = serde_json::from_str::<serde_json::Value>("not json");
        assert!(result.is_err());
    }

    // ---- TriggerStore with in-memory DB ----

    #[test]
    fn trigger_store_list_empty_initially() {
        let store = make_store();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn trigger_store_create_and_list() {
        let store = make_store();
        let trigger = store
            .create("my-trigger", "file_watch", "{}", "my-team", "")
            .unwrap();
        assert_eq!(trigger.name, "my-trigger");
        assert_eq!(trigger.trigger_type, "file_watch");
        assert_eq!(trigger.team_name, "my-team");
        assert!(trigger.enabled);
        assert_eq!(trigger.fire_count, 0);

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn trigger_store_get_by_name_returns_correct_trigger() {
        let store = make_store();
        store
            .create("alpha", "webhook_received", "{}", "team-a", "hello")
            .unwrap();
        store
            .create("beta", "message_received", "{}", "team-b", "")
            .unwrap();

        let found = store.get_by_name("alpha").unwrap().unwrap();
        assert_eq!(found.name, "alpha");
        assert_eq!(found.input, "hello");
    }

    #[test]
    fn trigger_store_get_by_name_returns_none_for_missing() {
        let store = make_store();
        assert!(store.get_by_name("missing").unwrap().is_none());
    }

    #[test]
    fn trigger_store_remove_existing_returns_true() {
        let store = make_store();
        store
            .create("to-remove", "file_watch", "{}", "team", "")
            .unwrap();
        assert!(store.remove("to-remove").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn trigger_store_remove_nonexistent_returns_false() {
        let store = make_store();
        assert!(!store.remove("ghost").unwrap());
    }

    #[test]
    fn trigger_store_set_enabled_disable_and_re_enable() {
        let store = make_store();
        store
            .create("toggle", "file_watch", "{}", "team", "")
            .unwrap();

        assert!(store.set_enabled("toggle", false).unwrap());
        let t = store.get_by_name("toggle").unwrap().unwrap();
        assert!(!t.enabled);

        assert!(store.set_enabled("toggle", true).unwrap());
        let t = store.get_by_name("toggle").unwrap().unwrap();
        assert!(t.enabled);
    }

    #[test]
    fn trigger_store_set_enabled_nonexistent_returns_false() {
        let store = make_store();
        assert!(!store.set_enabled("nonexistent", false).unwrap());
    }

    #[test]
    fn trigger_store_mark_fired_increments_count() {
        let store = make_store();
        store
            .create("fire-me", "webhook_received", "{}", "team", "")
            .unwrap();

        store.mark_fired("fire-me").unwrap();
        let t = store.get_by_name("fire-me").unwrap().unwrap();
        assert_eq!(t.fire_count, 1);
        assert!(t.last_fired_at.is_some());
    }

    #[test]
    fn trigger_store_condition_json_stored_and_retrieved() {
        let store = make_store();
        let condition = r#"{"channel":"general","user":"alice"}"#;
        store
            .create("cond-trigger", "message_received", condition, "team", "")
            .unwrap();

        let t = store.get_by_name("cond-trigger").unwrap().unwrap();
        assert_eq!(t.condition_json, condition);
    }

    #[test]
    fn trigger_store_list_by_type_filters_correctly() {
        let store = make_store();
        store.create("t1", "file_watch", "{}", "team", "").unwrap();
        store
            .create("t2", "webhook_received", "{}", "team", "")
            .unwrap();
        store.create("t3", "file_watch", "{}", "team", "").unwrap();

        let file_watch = store.list_by_type("file_watch").unwrap();
        assert_eq!(file_watch.len(), 2);
        let webhook = store.list_by_type("webhook_received").unwrap();
        assert_eq!(webhook.len(), 1);
    }

    // ---- CLI dispatch path tests via run() ----

    #[test]
    fn dispatch_list_empty_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();
        let result = run(TriggerAction::List, db, &team_store);
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_add_creates_trigger_in_db() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("alert-team");

        let result = run(
            TriggerAction::Add {
                name: "on-webhook".to_string(),
                trigger_type: "webhook_received".to_string(),
                team: "alert-team".to_string(),
                condition: r#"{"path":"/github/pr"}"#.to_string(),
                input: "handle PR event".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok(), "add should succeed: {result:?}");

        let store = TriggerStore::new(db);
        let t = store.get_by_name("on-webhook").unwrap().unwrap();
        assert_eq!(t.trigger_type, "webhook_received");
        assert_eq!(t.team_name, "alert-team");
        assert_eq!(t.input, "handle PR event");
        assert!(t.enabled);
    }

    #[test]
    fn dispatch_add_rejects_invalid_trigger_type() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("my-team");

        let result = run(
            TriggerAction::Add {
                name: "bad-type".to_string(),
                trigger_type: "kafka_event".to_string(),
                team: "my-team".to_string(),
                condition: "{}".to_string(),
                input: "".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("kafka_event"));
    }

    #[test]
    fn dispatch_add_rejects_invalid_condition_json() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("my-team");

        let result = run(
            TriggerAction::Add {
                name: "bad-json".to_string(),
                trigger_type: "file_watch".to_string(),
                team: "my-team".to_string(),
                condition: "not-json".to_string(),
                input: "".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid condition JSON")
        );
    }

    #[test]
    fn dispatch_add_rejects_nonexistent_team() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            TriggerAction::Add {
                name: "t".to_string(),
                trigger_type: "file_watch".to_string(),
                team: "missing-team".to_string(),
                condition: "{}".to_string(),
                input: "".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("missing-team"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn dispatch_remove_existing_trigger_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        TriggerStore::new(db.clone())
            .create("to-remove", "file_watch", "{}", "team", "")
            .unwrap();

        let result = run(
            TriggerAction::Remove {
                name: "to-remove".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok());

        assert!(
            TriggerStore::new(db)
                .get_by_name("to-remove")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn dispatch_remove_nonexistent_trigger_errors() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            TriggerAction::Remove {
                name: "ghost".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_enable_trigger_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let store = TriggerStore::new(db.clone());
        store.create("t", "file_watch", "{}", "team", "").unwrap();
        store.set_enabled("t", false).unwrap();

        let result = run(
            TriggerAction::Enable {
                name: "t".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok());

        let t = TriggerStore::new(db).get_by_name("t").unwrap().unwrap();
        assert!(t.enabled);
    }

    #[test]
    fn dispatch_enable_nonexistent_errors() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            TriggerAction::Enable {
                name: "no-such".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_disable_trigger_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        TriggerStore::new(db.clone())
            .create("active", "webhook_received", "{}", "team", "")
            .unwrap();

        let result = run(
            TriggerAction::Disable {
                name: "active".to_string(),
            },
            db.clone(),
            &team_store,
        );
        assert!(result.is_ok());

        let t = TriggerStore::new(db)
            .get_by_name("active")
            .unwrap()
            .unwrap();
        assert!(!t.enabled);
    }

    #[test]
    fn dispatch_disable_nonexistent_errors() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            TriggerAction::Disable {
                name: "no-such".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_status_existing_trigger_succeeds() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        TriggerStore::new(db.clone())
            .create("my-trigger", "message_received", "{}", "ops", "do stuff")
            .unwrap();

        let result = run(
            TriggerAction::Status {
                name: "my-trigger".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn dispatch_status_nonexistent_errors() {
        let db = make_db();
        let (_dir, team_store) = empty_team_store();

        let result = run(
            TriggerAction::Status {
                name: "missing".to_string(),
            },
            db,
            &team_store,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn dispatch_add_with_all_trigger_types_succeeds() {
        for trigger_type in [
            "file_watch",
            "message_received",
            "schedule_complete",
            "webhook_received",
        ] {
            let db = make_db();
            let (_dir, team_store) = make_team_store_with("test-team");

            let result = run(
                TriggerAction::Add {
                    name: format!("t-{trigger_type}"),
                    trigger_type: trigger_type.to_string(),
                    team: "test-team".to_string(),
                    condition: "{}".to_string(),
                    input: "".to_string(),
                },
                db,
                &team_store,
            );
            assert!(
                result.is_ok(),
                "add with type '{trigger_type}' should succeed"
            );
        }
    }

    #[test]
    fn dispatch_add_then_list_then_remove_lifecycle() {
        let db = make_db();
        let (_dir, team_store) = make_team_store_with("ops");

        run(
            TriggerAction::Add {
                name: "lifecycle".to_string(),
                trigger_type: "file_watch".to_string(),
                team: "ops".to_string(),
                condition: "{}".to_string(),
                input: "".to_string(),
            },
            db.clone(),
            &team_store,
        )
        .unwrap();

        run(TriggerAction::List, db.clone(), &team_store).unwrap();

        run(
            TriggerAction::Remove {
                name: "lifecycle".to_string(),
            },
            db.clone(),
            &team_store,
        )
        .unwrap();

        assert!(TriggerStore::new(db).list().unwrap().is_empty());
    }
}
