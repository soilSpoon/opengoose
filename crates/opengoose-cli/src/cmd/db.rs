use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_persistence::{Database, SessionStore};
use opengoose_profiles::ProfileStore;

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose db cleanup --profile main\n  opengoose db cleanup --retention-days 30\n  opengoose --json db cleanup --profile main"
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
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CleanupSummary {
    profile: String,
    retention_days: u32,
    deleted_messages: usize,
}

/// Dispatch and execute the selected database subcommand.
pub fn execute(action: DbAction, output: CliOutput) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let profile_store = ProfileStore::new()?;

    match action {
        DbAction::Cleanup {
            profile,
            retention_days,
        } => {
            let summary = run_cleanup(db, &profile_store, &profile, retention_days)?;
            if output.is_json() {
                output.print_json(&json!({
                    "ok": true,
                    "command": "db.cleanup",
                    "profile": summary.profile,
                    "retention_days": summary.retention_days,
                    "deleted_messages": summary.deleted_messages,
                }))?;
            } else {
                println!("Deleted {} expired message(s).", summary.deleted_messages);
                println!("  Profile: {}", summary.profile);
                println!("  Retention: {} day(s)", summary.retention_days);
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
) -> Result<CleanupSummary> {
    let retention_days = resolve_retention_days(profile_store, profile, retention_days_override)?;
    let deleted_messages = SessionStore::new(db).cleanup_expired_messages(retention_days)?;

    Ok(CleanupSummary {
        profile: profile.to_string(),
        retention_days,
        deleted_messages,
    })
}

fn resolve_retention_days(
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

    fn save_profile(store: &ProfileStore, name: &str, retention_days: Option<u32>) {
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
                    settings: retention_days.map(|days| ProfileSettings {
                        message_retention_days: Some(days),
                        ..ProfileSettings::default()
                    }),
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
    fn resolve_retention_days_prefers_cli_override() {
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", Some(14));

        let days = resolve_retention_days(&store, "main", Some(30)).unwrap();

        assert_eq!(days, 30);
    }

    #[test]
    fn resolve_retention_days_uses_profile_setting() {
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", Some(14));

        let days = resolve_retention_days(&store, "main", None).unwrap();

        assert_eq!(days, 14);
    }

    #[test]
    fn resolve_retention_days_errors_when_unconfigured() {
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", None);

        let err = resolve_retention_days(&store, "main", None).unwrap_err();

        assert!(
            err.to_string()
                .contains("does not configure message retention")
        );
    }

    #[test]
    fn run_cleanup_deletes_old_messages() {
        let db = db_with_expired_messages();
        let (_dir, store) = temp_profile_store();
        save_profile(&store, "main", Some(0));

        let summary = run_cleanup(db.clone(), &store, "main", None).unwrap();

        assert_eq!(
            summary,
            CleanupSummary {
                profile: "main".into(),
                retention_days: 0,
                deleted_messages: 2,
            }
        );
        let history = SessionStore::new(db)
            .load_history(&session_key(), 10)
            .unwrap();
        assert!(history.is_empty());
    }
}
