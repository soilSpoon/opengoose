use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;

use opengoose_persistence::{AlertStore, Database};

mod create;
mod health;
mod history;
mod list;
mod mutate;

#[cfg(test)]
mod tests;

#[derive(Subcommand)]
/// Subcommands for `opengoose alert`.
pub enum AlertAction {
    /// List all alert rules
    List,
    /// Create a new alert rule
    Create {
        /// Rule name (must be unique)
        name: String,
        /// Metric to monitor: queue_backlog, failed_runs, error_rate
        #[arg(long, short)]
        metric: String,
        /// Condition operator: gt, lt, gte, lte
        #[arg(long, short)]
        condition: String,
        /// Threshold value
        #[arg(long, short)]
        threshold: f64,
        /// Optional description
        #[arg(long, short)]
        description: Option<String>,
    },
    /// Delete an alert rule by name
    Delete {
        /// Rule name
        name: String,
    },
    /// Enable an alert rule
    Enable {
        /// Rule name
        name: String,
    },
    /// Disable an alert rule
    Disable {
        /// Rule name
        name: String,
    },
    /// Run a health check: evaluate all enabled rules against current system metrics
    Test,
    /// Show recent alert history
    History {
        /// Number of entries to show
        #[arg(long, default_value = "20")]
        limit: i64,
    },
}

/// Dispatch and execute the selected alert subcommand.
pub fn execute(action: AlertAction) -> Result<()> {
    let store = open_store()?;
    run(action, &store)
}

pub(crate) fn run(action: AlertAction, store: &AlertStore) -> Result<()> {
    match action {
        AlertAction::List => list::run(store),
        AlertAction::Create {
            name,
            metric,
            condition,
            threshold,
            description,
        } => create::run(
            store,
            &name,
            &metric,
            &condition,
            threshold,
            description.as_deref(),
        ),
        AlertAction::Delete { name } => mutate::delete(store, &name),
        AlertAction::Enable { name } => mutate::set_enabled(store, &name, true),
        AlertAction::Disable { name } => mutate::set_enabled(store, &name, false),
        AlertAction::Test => health::run(store),
        AlertAction::History { limit } => history::run(store, limit),
    }
}

fn open_store() -> Result<AlertStore> {
    let db = Arc::new(Database::open()?);
    Ok(AlertStore::new(db))
}
