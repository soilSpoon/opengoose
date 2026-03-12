use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;

use opengoose_persistence::{Database, PluginStore};

mod commands;

#[cfg(test)]
mod tests;

#[derive(Subcommand)]
/// Subcommands for `opengoose plugin`.
pub enum PluginAction {
    /// Install a plugin from a local path
    Install {
        /// Path to the plugin directory (must contain plugin.toml)
        path: PathBuf,
    },
    /// List all installed plugins
    List,
    /// Remove an installed plugin by name
    Remove {
        /// Plugin name
        name: String,
    },
    /// Show information about a plugin
    Info {
        /// Plugin name
        name: String,
    },
    /// Enable a plugin
    Enable {
        /// Plugin name
        name: String,
    },
    /// Disable a plugin
    Disable {
        /// Plugin name
        name: String,
    },
    /// Scan the plugins directory and show discovered (not yet installed) plugins
    Discover,
}

/// Dispatch and execute the selected plugin subcommand.
pub fn execute(action: PluginAction) -> Result<()> {
    let db = Arc::new(Database::open()?);
    run(action, db)
}

/// Testable dispatch: accepts injected db.
pub(crate) fn run(action: PluginAction, db: Arc<Database>) -> Result<()> {
    let store = PluginStore::new(db.clone());
    match action {
        PluginAction::Install { path } => commands::install(db, path),
        PluginAction::List => commands::list(&store),
        PluginAction::Remove { name } => commands::remove(db, &name),
        PluginAction::Info { name } => commands::info(&store, &name),
        PluginAction::Enable { name } => commands::enable(db, &name),
        PluginAction::Disable { name } => commands::disable(db, &name),
        PluginAction::Discover => commands::discover(&store),
    }
}
