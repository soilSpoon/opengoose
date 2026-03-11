use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_persistence::{ApiKeyStore, Database};

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose api-key generate --description \"CI pipeline\"\n  opengoose api-key list\n  opengoose api-key revoke <KEY_ID>\n  opengoose --json api-key list"
)]
/// Subcommands for `opengoose api-key`.
pub enum ApiKeyAction {
    /// Generate a new API key
    Generate {
        /// Human-readable description for the key
        #[arg(long)]
        description: Option<String>,
    },
    /// List all API keys (without secret material)
    List,
    /// Revoke an API key by its ID
    Revoke {
        /// The key ID to revoke
        key_id: String,
    },
}

/// Dispatch and execute the selected api-key subcommand.
pub fn execute(action: ApiKeyAction, output: CliOutput) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = ApiKeyStore::new(db);

    match action {
        ApiKeyAction::Generate { description } => {
            let key = store.generate(description.as_deref())?;
            if output.is_json() {
                output.print_json(&json!({
                    "ok": true,
                    "command": "api-key.generate",
                    "id": key.id,
                    "key": key.plaintext,
                    "description": key.description,
                }))?;
            } else {
                println!("Generated API key:");
                println!("  ID:  {}", key.id);
                println!("  Key: {}", key.plaintext);
                if let Some(desc) = &key.description {
                    println!("  Description: {desc}");
                }
                println!();
                println!("Save this key now — it cannot be retrieved later.");
            }
            Ok(())
        }
        ApiKeyAction::List => {
            let keys = store.list()?;
            if output.is_json() {
                output.print_json(&json!({
                    "ok": true,
                    "command": "api-key.list",
                    "keys": keys,
                }))?;
            } else if keys.is_empty() {
                println!("No API keys found.");
            } else {
                println!(
                    "{:<38} {:<30} {:<22} {}",
                    "ID", "DESCRIPTION", "CREATED", "LAST USED"
                );
                for k in &keys {
                    println!(
                        "{:<38} {:<30} {:<22} {}",
                        k.id,
                        k.description.as_deref().unwrap_or("-"),
                        &k.created_at,
                        k.last_used_at.as_deref().unwrap_or("never"),
                    );
                }
            }
            Ok(())
        }
        ApiKeyAction::Revoke { key_id } => {
            let revoked = store.revoke(&key_id)?;
            if output.is_json() {
                output.print_json(&json!({
                    "ok": revoked,
                    "command": "api-key.revoke",
                    "key_id": key_id,
                }))?;
            } else if revoked {
                println!("API key {key_id} revoked.");
            } else {
                println!("No API key found with ID {key_id}.");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        action: ApiKeyAction,
    }

    #[test]
    fn parse_generate_with_description() {
        let cli = TestCli::parse_from(["test", "generate", "--description", "CI key"]);
        match cli.action {
            ApiKeyAction::Generate { description } => {
                assert_eq!(description.as_deref(), Some("CI key"));
            }
            _ => panic!("expected Generate"),
        }
    }

    #[test]
    fn parse_generate_without_description() {
        let cli = TestCli::parse_from(["test", "generate"]);
        match cli.action {
            ApiKeyAction::Generate { description } => {
                assert!(description.is_none());
            }
            _ => panic!("expected Generate"),
        }
    }

    #[test]
    fn parse_list() {
        let cli = TestCli::parse_from(["test", "list"]);
        assert!(matches!(cli.action, ApiKeyAction::List));
    }

    #[test]
    fn parse_revoke() {
        let cli = TestCli::parse_from(["test", "revoke", "some-uuid"]);
        match cli.action {
            ApiKeyAction::Revoke { key_id } => {
                assert_eq!(key_id, "some-uuid");
            }
            _ => panic!("expected Revoke"),
        }
    }
}
