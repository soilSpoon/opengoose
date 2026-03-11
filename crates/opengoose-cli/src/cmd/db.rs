use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_persistence::{DEFAULT_EVENT_RETENTION_DAYS, Database, EventStore, SessionStore};
use opengoose_profiles::ProfileStore;

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose db cleanup --profile main\n  opengoose db cleanup --retention-days 30 --event-retention-days 14\n  opengoose --json db cleanup --profile main"
)]
/// Subcommands for `opengoose db`.
pub enum DbAction {
    /// Delete persisted session messages older than the configured retention window
    Cleanup {
        /// Profile whose configured retention should be used
        #[arg(long, default_value = "main")]
        profile: String,
        /// Override the profile setting for this cleanup run
        #[arg(long)]
        retention_days: Option<u32>,
        /// Override event history retention for this cleanup run
        #[arg(long)]
        event_retention_days: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CleanupSummary {
    profile: String,
    message_retention_days: u32,
    event_retention_days: u32,
    deleted_messages: usize,
    deleted_events: usize,
}

/// Dispatch and execute the selected database subcommand.
pub fn execute(action: DbAction, output: CliOutput) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let profile_store = ProfileStore::new()?;

    match action {
        DbAction::Cleanup {
            profile,
            retention_days,
            event_retention_days,
        } => {
            let summary = run_cleanup(
                db,
                &profile_store,
                &profile,
                retention_days,
                event_retention_days,
            )?;
            if output.is_json() {
                output.print_json(&json!({
                    "ok": true,
                    "command": "db.cleanup",
                    "profile": summary.profile,
                    "message_retention_days": summary.message_retention_days,
                    "event_retention_days": summary.event_retention_days,
                    "deleted_messages": summary.deleted_messages,
                    "deleted_events": summary.deleted_events,
                }))?;
            } else {
                println!("Deleted {} expired message(s).", summary.deleted_messages);
                println!("Deleted {} expired event(s).", summary.deleted_events);
                println!("  Profile: {}", summary.profile);
                println!(
                    "  Message retention: {} day(s)",
                    summary.message_retention_days
                );
                println!("  Event retention: {} day(s)", summary.event_retention_days);
            }
            Ok(())
        }
    }
}

fn run_cleanup(
    db: Arc<Database>,
    profile_store: &ProfileStore,
    profile: &str,
    retention_days_override: Option<u32>,
    event_retention_days_override: Option<u32>,
) -> Result<CleanupSummary> {
    let message_retention_days =
        resolve_message_retention_days(profile_store, profile, retention_days_override)?;
    let event_retention_days =
        resolve_event_retention_days(profile_store, profile, event_retention_days_override)?;
    let deleted_messages =
        SessionStore::new(db.clone()).cleanup_expired_messages(message_retention_days)?;
    let deleted_events = EventStore::new(db).cleanup_expired(event_retention_days)?;

    Ok(CleanupSummary {
        profile: profile.to_string(),
        message_retention_days,
        event_retention_days,
        deleted_messages,
        deleted_events,
    })
}

fn resolve_message_retention_days(
    profile_store: &ProfileStore,
    profile: &str,
    retention_days_override: Option<u32>,
) -> Result<u32> {
    if let Some(retention_days) = retention_days_override {
        return Ok(retention_days);
    }

    let profile = profile_store.get(profile)?;
    if let Some(retention_days) = profile
        .settings
        .and_then(|settings| settings.message_retention_days)
    {
        Ok(retention_days)
    } else {
        bail!(
            "profile `{}` does not configure message retention. Run `opengoose profile set {} --message-retention-days <N>` or pass `--retention-days`.",
            profile.title,
            profile.title
        );
    }
}

fn resolve_event_retention_days(
    profile_store: &ProfileStore,
    profile: &str,
    retention_days_override: Option<u32>,
) -> Result<u32> {
    if let Some(retention_days) = retention_days_override {
        return Ok(retention_days);
    }

    let profile = profile_store.get(profile)?;
    Ok(profile
        .settings
        .and_then(|settings| settings.event_retention_days)
        .unwrap_or(DEFAULT_EVENT_RETENTION_DAYS))
}

#[cfg(test)]
mod tests {
    use super::*;

    use opengoose_profiles::{AgentProfile, ProfileSettings};
    use opengoose_types::{Platform, SessionKey};

    fn temp_profile_store() -> (tempfile::TempDir, ProfileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::with_dir(dir.path().to_path_buf());
        (dir, store)
    }

    fn save_profile(
        store: &ProfileStore,
        name: &str,
        message_retention_days: Option<u32>,
        event_retention_days: Option<u32>,
    ) {
        store
            .save(
                &AgentProfile {
                    version: "1.0.0".into(),
                    title: name.into(),
                    description: None,
                    instructions: None,
                    prompt: None,
                    extensions: vec![],
                    skills: vec![],
                    settings: if message_retention_days.is_none() && event_retention_days.is_none()
                    {
                        None
                    } else {
                        Some(ProfileSettings {
                            message_retention_days,
                            event_retention_days,
                            ..ProfileSettings::default()
                        })
                    },
                    activities: None,
                    response: None,
                    sub_recipes: None,
                    parameters: None,
                },
                false,
            )
            .unwrap();
    }

    fn session_key() -> SessionKey {
        SessionKey::new(Platform::Discord, "guild".to_string(), "channel")
    }

    fn db_with_expired_messages() -> Arc<Database> {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let store = SessionStore::new(db.clone());
        let key = session_key();
        store.append_user_message(&key, "first", None).unwrap();
        store.append_user_message(&key, "second", None).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        db
    }

    #[test]
    fn resolve_message_retention_days_prefers_cli_override() {
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", Some(14), Some(21));

        let days = resolve_message_retention_days(&store, "main", Some(30)).unwrap();

        assert_eq!(days, 30);
    }

    #[test]
    fn resolve_message_retention_days_uses_profile_setting() {
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", Some(14), Some(21));

        let days = resolve_message_retention_days(&store, "main", None).unwrap();

        assert_eq!(days, 14);
    }

    #[test]
    fn resolve_message_retention_days_errors_when_unconfigured() {
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", None, Some(21));

        let err = resolve_message_retention_days(&store, "main", None).unwrap_err();

        assert!(
            err.to_string()
                .contains("does not configure message retention")
        );
    }

    #[test]
    fn resolve_event_retention_days_prefers_cli_override() {
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", Some(14), Some(21));

        let days = resolve_event_retention_days(&store, "main", Some(7)).unwrap();

        assert_eq!(days, 7);
    }

    #[test]
    fn resolve_event_retention_days_defaults_when_unconfigured() {
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", Some(14), None);

        let days = resolve_event_retention_days(&store, "main", None).unwrap();

        assert_eq!(days, DEFAULT_EVENT_RETENTION_DAYS);
    }

    #[test]
    fn run_cleanup_deletes_old_messages() {
        let db = db_with_expired_messages();
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", Some(0), Some(30));

        let summary = run_cleanup(db.clone(), &store, "main", None, None).unwrap();

        assert_eq!(
            summary,
            CleanupSummary {
                profile: "main".into(),
                message_retention_days: 0,
                event_retention_days: 30,
                deleted_messages: 2,
                deleted_events: 0,
            }
        );
        let history = SessionStore::new(db)
            .load_history(&session_key(), 10)
            .unwrap();
        assert!(history.is_empty());
    }
}
